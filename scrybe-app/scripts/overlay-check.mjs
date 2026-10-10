// SPDX-License-Identifier: Apache-2.0
import {readFile,readdir} from 'node:fs/promises';
import {resolve,dirname,extname,relative} from 'node:path';
import {fileURLToPath} from 'node:url';
import {gzipSync} from 'node:zlib';
import ts from 'typescript';
const app=fileURLToPath(new URL('../',import.meta.url));
const output=resolve(app,process.argv[2]??'dist-overlay');
const failures=[];
// An optional fixture file lets tests exercise policy without changing the
// checked-in exceptions. Production/CI always use the adjacent default file.
const allowedFile=process.argv[3]?resolve(process.argv[3]):fileURLToPath(new URL('./overlay-allowed-urls.json',import.meta.url));
const entries=JSON.parse(await readFile(allowedFile,'utf8'));
if(!Array.isArray(entries))throw Error('URL exceptions must be an array');
const allowed=new Map();
for(const entry of entries){
  if(!entry || typeof entry.url!=='string' || typeof entry.package!=='string' || !entry.package.trim() || typeof entry.reason!=='string' || !entry.reason.trim())throw Error('Each URL exception needs url, package and reason');
  const url=new URL(entry.url);
  if(!['http:','https:'].includes(url.protocol))throw Error('URL exception must be HTTP(S)');
  if(allowed.has(entry.url))throw Error('Duplicate URL exception: '+entry.url);
  allowed.set(entry.url,entry);
}
const seenURLs=new Set();
function absoluteURLs(text){
  const urls=[];
  for(const match of text.matchAll(/https?:\/\/[^\s"'`<>\\]+/gi)){
    let url=match[0];
    // Error prose often wraps a documentation pointer in parentheses. Keep
    // balanced URL parentheses; only explicit "(see URL)" error prose has a
    // surrounding delimiter. A URL literal ending in ) remains distinct.
    while(text.slice(Math.max(0,match.index-5),match.index)==='(see ' && url.endsWith(')') && (url.match(/\)/g)?.length??0)>(url.match(/\(/g)?.length??0))url=url.slice(0,-1);
    urls.push(url);
  }
  return urls;
}
function artifactURLs(text,name){
  const urls=new Set(absoluteURLs(text));
  if(name.endsWith('.js')){
    const ast=ts.createSourceFile(name,text,ts.ScriptTarget.Latest,true,ts.ScriptKind.JS);
    function visit(node){
      // Decode escaped JS literals too. For interpolated diagnostics retain
      // the exact emitted expression spelling; no wildcard/prefix matching.
      if(ts.isStringLiteral(node)||ts.isNoSubstitutionTemplateLiteral(node))for(const url of absoluteURLs(node.text))urls.add(url);
      if(ts.isTemplateExpression(node)){
        let value=node.head.text;
        for(const span of node.templateSpans)value+='${'+span.expression.getText(ast)+'}'+span.literal.text;
        for(const url of absoluteURLs(value))urls.add(url);
      }
      ts.forEachChild(node,visit);
    }
    visit(ast);
  }
  return urls;
}
const forbidden=['home'+'.lab','home'+'.lan','192'+'.168.','/Use'+'rs/','my'+'_home','claude'+'.ai'];
async function walk(dir){return (await Promise.all((await readdir(dir,{withFileTypes:true})).map(async e=>e.isDirectory()?walk(resolve(dir,e.name)):[resolve(dir,e.name)]))).flat();}
const files=await walk(output);
const manifest=JSON.parse(await readFile(resolve(output,'.vite/manifest.json'),'utf8'));
if(!manifest['overlay/frame.ts']?.isEntry)failures.push('Missing frame entry manifest');
if(!manifest['overlay/editor.ts']?.isDynamicEntry)failures.push('Missing lazy editor manifest');
if(!files.includes(resolve(output,'overlay.js')))failures.push('Missing classic loader');
let cssSize=0;
for(const file of files){
  const bytes=await readFile(file),text=bytes.toString('utf8'),name=relative(output,file),size=gzipSync(bytes).length;
  if(name.endsWith('.css'))cssSize+=size;
  const budget=name==='overlay.js'?15*1024:name===manifest['overlay/frame.ts']?.file?30*1024:name.endsWith('.js')?300*1024:Infinity;
  if(size>budget)failures.push(`${name}: ${size} exceeds ${budget} gzip bytes`);
  if(/\.(?:js|css)$/.test(name))console.log(`${name}: ${size} gzip bytes`);
  const checks=[[/\beval\s*\(/,'eval'],[/\bnew\s+Function\b/,'new Function'],[/<script(?:\s|>)/i,'inline script'],[/@font-face[^}]*data:/is,'data font'],[/data:(?:font\/|application\/(?:font|x-font|vnd\.ms-fontobject))/i,'data font'],[/["'`(]\/assets\//,'root-relative asset']];
  for(const [pattern,label] of checks)if(pattern.test(text))failures.push(`${name}: ${label}`);
  for(const url of artifactURLs(text,name)){seenURLs.add(url);if(!allowed.has(url))failures.push(`${name}: unlisted absolute HTTP(S) URL: ${url}`);}
  for(const word of forbidden)if(text.includes(word))failures.push(`${name}: forbidden private string`);
}
for(const url of allowed.keys())if(!seenURLs.has(url))failures.push(`Stale URL exception: ${url}`);
console.log(`URL exceptions: ${allowed.size}; absolute URLs observed: ${seenURLs.size}`);
if(cssSize>30*1024)failures.push(`CSS total ${cssSize} exceeds 30720 gzip bytes`);
const pkg=JSON.parse(await readFile(resolve(app,'package.json'),'utf8'));
const declared=new Set(Object.keys({...pkg.dependencies,...pkg.devDependencies}));
const visited=new Set();
async function inspect(file){
  file=resolve(file);if(visited.has(file))return;visited.add(file);
  const source=await readFile(file,'utf8');
  if(!/\.(?:ts|js|mjs|css)$/.test(file))return;
  const imports=[];
  if(extname(file)==='.css'){for(const match of source.matchAll(/@import\s+['"]([^'"]+)['"]/g))imports.push(match[1]);}
  else {const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);function visit(node){if((ts.isImportDeclaration(node)||ts.isExportDeclaration(node))&&node.moduleSpecifier&&ts.isStringLiteral(node.moduleSpecifier))imports.push(node.moduleSpecifier.text);if(ts.isCallExpression(node)&&node.expression.kind===ts.SyntaxKind.ImportKeyword&&ts.isStringLiteral(node.arguments[0]))imports.push(node.arguments[0].text);ts.forEachChild(node,visit);}visit(ast);}
  for(const spec of imports){
    if(spec.startsWith('node:'))continue;
    if(spec.startsWith('.')){let next=resolve(dirname(file),spec);if(!extname(next))next+='.ts';await inspect(next);}
    else {const name=spec.startsWith('@')?spec.split('/').slice(0,2).join('/'):spec.split('/')[0];if(!declared.has(name))failures.push(`${relative(app,file)} imports undeclared ${name}`);else{try{import.meta.resolve(spec);}catch{failures.push(`Unresolvable import ${spec}`);}}}
  }
}
for(const file of await walk(resolve(app,'overlay')))if(/\.(?:ts|js|mjs|css)$/.test(file))await inspect(file);
console.log(`CSS total: ${cssSize} gzip bytes; import closure: ${visited.size} project files`);
if(failures.length){console.error(failures.join('\n'));process.exitCode=1;}else console.log('Overlay checks passed.');
