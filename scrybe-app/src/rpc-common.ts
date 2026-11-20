// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";

// ─── Phase 2: read-side commands with reply correlation ────────────────────
//
// These events carry an `id` field that the server is blocking on. The
// frontend handler calls `cli_rpc_reply(id, {result|error})` to unblock
// the dispatcher, which packages the payload into a JSON-RPC response.

export interface ReplyOk { result: unknown }
export interface ReplyErr { error: { code: number; message: string } }

export async function reply(id: number, body: ReplyOk | ReplyErr): Promise<void> {
  await invoke("cli_rpc_reply", { id, reply: body });
}

export const ERR_TAB_NOT_OPEN = -32001;
export const ERR_SECTION_NOT_FOUND = -32004;
export const ERR_DIRTY_RELOAD_REFUSED = -32005;
