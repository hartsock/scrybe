// SPDX-License-Identifier: Apache-2.0
import {execFileSync} from 'node:child_process';
import {micromark} from 'micromark';
import {gfm,gfmHtml} from 'micromark-extension-gfm';
const ruby=`gem 'kramdown', '2.4.0'; require 'json'; require 'kramdown'; require 'kramdown-parser-gfm'; puts JSON.generate(JSON.parse(STDIN.read).map { |s| Kramdown::Document.new(s,input:'GFM',hard_wrap:false,auto_ids:true,syntax_highlighter:nil).to_html })`;
export const kramdown=texts=>JSON.parse(execFileSync('ruby',['-e',ruby],{input:JSON.stringify(texts),encoding:'utf8',maxBuffer:128*1024*1024,stdio:['pipe','pipe','pipe']}));
// Paragraph-agnostic: list looseness is intentionally outside this block oracle.
export const skeleton=html=>(html.match(/<\/?(h[1-6]|ul|ol|li|blockquote|pre|table|tr|hr)\b/g)??[]).join('');
export const commonmark=s=>micromark(s,{extensions:[gfm()],htmlExtensions:[gfmHtml()],allowDangerousHtml:true});
