// SPDX-License-Identifier: Apache-2.0
import {build} from 'vite';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
await build({configFile:false,publicDir:false,root,base:'./',build:{outDir:'dist-overlay',emptyOutDir:true,target:'es2022',modulePreload:{polyfill:false},manifest:true,rollupOptions:{input:{frame:'overlay/frame.ts'},output:{entryFileNames:'assets/[name]-[hash].js',chunkFileNames:'assets/[name]-[hash].js',assetFileNames:'assets/[name]-[hash][extname]'}}}});
await build({configFile:false,publicDir:false,root,base:'./',build:{outDir:'dist-overlay',emptyOutDir:false,target:'es2022',lib:{entry:'overlay/loader.ts',name:'ScrybeOverlay',formats:['iife'],fileName:()=> 'overlay.js'}}});
