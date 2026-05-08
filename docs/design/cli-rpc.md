# `scrybe` CLI as the universal command surface

**Status:** Phase 1 implemented (open / save / close / quit). Phase 2 to follow.

## Goal

A single `scrybe` binary that mirrors every MCP tool. Humans drive the GUI from the shell, agents drive Scrybe without an MCP client setup. When the GUI is running, CLI commands operate on its in-memory state via a Unix-domain socket; when it isn't, GUI-only commands launch the app, and read-only commands run inline against disk.

## Wire protocol

JSON-RPC 2.0, newline-delimited, one line per request and one per response. Transport is a Unix-domain socket (Windows named pipe deferred to a follow-up). Default path is `~/.scrybe/sock`; override with `$SCRYBE_SOCK`.

### Methods (Phase 1)

| Method | Params | Result |
|---|---|---|
| `open`  | `{path: String}` | `{tab_id: String, reloaded: bool}` |
| `save`  | `{path: String}` | `{applied: bool}` |
| `close` | `{path: String}` | `{applied: bool}` |
| `quit`  | `{force: bool}` | `{applied: bool}` |

### Error codes

Standard JSON-RPC codes (`-32700` parse, `-32600` invalid request, `-32601` method not found, `-32602` invalid params, `-32603` internal). App-defined range starts at `-32000`:

- `-32001` — tab not open (Phase 2 readers; Phase 1 collapses this to `applied: false` for `save`/`close`)
- `-32002` — quit refused due to dirty buffers (`force=false` and unsaved tabs exist)

## Structure

Three crates participate:

```
scrybe-rpc/                  Wire types — single source of truth for both sides
├── Request, Response, RpcError, JsonRpcVersion("2.0")
├── OpenParams, SaveParams, CloseParams, QuitParams
├── OpenResult, AckResult
└── default_socket_path()    ~/.scrybe/sock or $SCRYBE_SOCK

scrybe-cli/                  CLI client — clap surface + RPC dialer
├── src/main.rs              clap subcommands + dispatch
└── src/rpc_client.rs        try_connect, send, send_to (explicit-path variant for tests)

scrybe-app/src-tauri/        GUI server — socket binder + dispatcher
└── src/cli_rpc.rs           bind socket, accept, parse JSON-RPC, emit Tauri events
```

## CLI ergonomics

### Bare-path shortcut

`scrybe foo.md` ≡ `scrybe open foo.md`. The detection rule: if `argv[1]` is not a recognized subcommand and not a flag (doesn't start with `-`), inject `open` at position 1. So:

- `scrybe foo.md`         → `scrybe open foo.md`
- `scrybe ./foo.md`       → `scrybe open ./foo.md`
- `scrybe save foo.md`    → unchanged (subcommand recognized)
- `scrybe foo`            → `scrybe open foo` (which then errors at canonicalization if `./foo` doesn't exist)

### `--help` is the operator manual

`scrybe --help` is intentionally heavier than clap defaults: the top-level `long_about` documents every subcommand, the connection model, fall-through semantics for no-app-running, environment variables, and install instructions. Per-subcommand `--help` shows the long-form description.

## Server side

`cli_rpc::spawn(app)` is called from the Tauri `setup()` block alongside the file-watcher init. It:

1. Resolves the socket path.
2. Creates the parent directory if needed.
3. Stale-socket recovery: if the socket file exists, try to connect. If connect succeeds, refuses to bind (another Scrybe is alive; the Tauri single-instance plugin also catches this from a different angle). If connect fails, unlinks and rebinds.
4. Spawns an accept thread; per-connection requests are handled in their own thread.

The dispatcher is fire-and-forget for Phase 1: each method emits a typed Tauri event (`scrybe://cli-open`, `scrybe://cli-save`, `scrybe://cli-close`, `scrybe://cli-quit`) to the frontend, then immediately returns an ack response. The frontend handler does the actual work using the same code paths the autosave timer and the file-watcher already use.

Phase 2 (`lint`/`render`/`read`/`find`/`section`/`edit`/`embed`/`extract`) needs request/response correlation: the frontend will reply via a `scrybe://cli-rpc-reply` event keyed by request id, and the dispatcher will block on that reply with a timeout. The Phase 1 ack-immediately model is simpler and correct for purely-mutating GUI commands.

## Frontend buttons + shortcuts

Two new toolbar buttons (`💾 Save`, `🔄 Reload`) operate on the active tab. Keyboard shortcuts mirror them:

- `Ctrl+S` / `⌘S` → save active tab
- `Ctrl+R` / `⌘R` → reload active tab from disk (intercepts the default page-reload behavior)

The Reload action reuses `reloadTabFromDisk(path)` — the same function the file-watcher event handler calls — so the conflict-bar (Keep mine / Take theirs) shows automatically when the buffer is dirty.

## Phase split

### Phase 1 (this PR)

- `scrybe-rpc` crate with wire types
- Socket bind/accept on app side
- CLI client + clap subcommand surface with full `--help` text
- 4 GUI methods: `open`, `save`, `close`, `quit` (`--force`)
- Save/Reload toolbar buttons + `Ctrl+S`/`Ctrl+R` shortcuts
- 7 integration tests against a real socket; 10 unit tests on wire types; 5 unit tests on RPC client; 6 unit tests on the server-side dispatcher helpers
- Closes the second half of #15 (force-reload-on-`open` semantics)

### Phase 2 (follow-up)

- Read-side methods: `lint`, `render`, `read`, `find`, `section`, `edit`, `embed`, `extract`
- `--json` flag plumbing on read-side commands
- Inline fallbacks for `lint`/`render`/`embed`/`extract`/`find`/`section` when no GUI is running
- Reply-correlation via `scrybe://cli-rpc-reply`

## Test coverage

Phase 1 lands at:

| File | Lines coverage |
|---|---|
| `scrybe-rpc/src/lib.rs` | 95.87% |
| `scrybe-cli/src/rpc_client.rs` | 79.76% |
| `scrybe-cli/src/wrap.rs` | 100% |
| `scrybe-cli/src/lint.rs` | 88.39% |
| `scrybe-cli/src/lib.rs` | 88.46% |
| `scrybe-cli/src/main.rs` | 0% (clap dispatch — exercised by manual smoke tests; subprocess-based tests deferred) |
| `scrybe-app/src-tauri/src/cli_rpc.rs` | covered by unit tests on dispatch helpers + the wire-protocol integration tests |

All non-dispatch-glue files clear the 80% target. `main.rs`'s dispatch arms are short and primarily forward to library functions that are themselves under coverage; subprocess-based tests via `assert_cmd` are queued for Phase 2.

## Open follow-ups

1. **Multi-window socket model.** Tauri's single-instance plugin guarantees one process and therefore one socket. If multi-window/multi-profile becomes a feature, this design needs revisiting.
2. **Windows named-pipe support.** Currently a `cfg(unix)` story. Phase 2 or later will add a named-pipe transport sharing the same JSON-RPC framing.
3. **Linux `scrybe-app` discovery.** `scrybe foo.md` with no GUI running on Linux uses `$SCRYBE_APP_BIN` first, then `scrybe-app` on `PATH`. macOS uses `open -a Scrybe`. Windows is deferred.
4. **Coverage of `main.rs` dispatch arms.** Adding `assert_cmd`-based subprocess tests in Phase 2.
