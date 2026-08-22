<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-rpc

JSON-RPC 2.0 wire protocol shared between the Scrybe CLI and the running
Scrybe desktop app. Defines the request/response/error envelopes and method
names; both sides import this crate so the protocol has a single source of
truth.

## What it does

When the desktop app is running, the CLI talks to it instead of opening
files directly. That makes commands like `scrybe open path/to/note.md`
surface in the existing GUI session as a new tab. The conversation between
the two processes goes over a Unix-domain socket or Windows named pipe
using newline-delimited JSON-RPC 2.0.

This crate owns the typed protocol, shared client, and platform transport so
the CLI, MCP server, and desktop app cannot drift into different wire paths.

## Methods (Phase 1)

| Method | Effect |
|---|---|
| `open(path)` | Open a tab, or force-refresh if the file is already open |
| `save(path)` | Save an open tab's buffer to disk; no-op if not open |
| `close(path)` | Close a tab; no-op if not open |
| `quit({ force })` | Quit the app; `force=true` skips the dirty-buffer prompt |

## Framing

Newline-delimited JSON. One request per line, one response per line.
Multiple requests on a single connection are processed FIFO.

## Local endpoint

Unix uses `~/.scrybe/sock`. Windows uses a per-user
`\\.\pipe\scrybe-<user>` named pipe. Override either default with the
`SCRYBE_SOCK` environment variable; on Windows the override must also be a
full `\\.\pipe\...` named-pipe path.

## License

Apache-2.0. See the [LICENSE](https://github.com/hartsock/scrybe/blob/main/LICENSE)
file at the workspace root.
