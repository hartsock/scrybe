// SPDX-License-Identifier: Apache-2.0
// Test instrumentation only: the frame still mounts the real editor and keymaps.
import * as real from '../../overlay/editor.ts';
import {editorViewCtx} from '@milkdown/kit/core';
export const {placeCaret,cleanPaste,trackSource}=real;
let mounted;
export async function mountEditor(root,value){mounted=await real.mountEditor(root,value);return mounted;}
export const view=()=>mounted.editor.action(ctx=>ctx.get(editorViewCtx));
