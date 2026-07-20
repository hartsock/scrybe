# Scrybe socket RPC contract — 0.6 compatibility artifact

> **This file is the 0.6 compatibility artifact.** It freezes the wire
> contract between the `scrybe` CLI / MCP server (clients) and the running
> Scrybe app (server) for the 0.6 line. Anything documented here — method
> names, param/result shapes, error codes, envelope rules, limits — may only
> change with a contract version bump and a new fixture. The design narrative
> lives in `docs/design/cli-rpc.md`; this file is the normative reference.
>
> Source of truth in code: `scrybe-rpc/src/lib.rs` (wire types + error-code
> registry) and `scrybe-rpc/src/client.rs` (client-side validation). The
> error-code registry is enforced by the
> `app_error_codes_unique_and_in_reserved_range` test; the client validation
> rules are enforced by `scrybe-rpc/tests/wire_contract.rs`.

## Transport & framing

- **Transport:** Unix-domain socket at `~/.scrybe/sock` (override:
  `$SCRYBE_SOCK`). Windows named pipes are out of scope for 0.6.
- **Framing:** newline-delimited JSON-RPC 2.0. One request per line, one
  response per line. The reference client opens **one connection per
  request** and always uses request id `1`; the server MUST echo the request
  id on the response.
- **Frame cap:** a reply line (including its trailing newline) may be at most
  **16 MiB** (`scrybe_rpc::client::MAX_FRAME_BYTES`). The client stops
  reading at the cap and reports `FrameTooLarge` — it never allocates
  unboundedly.
- **Timeouts:** the client applies a 5 s read timeout and a 5 s write timeout
  (`READ_TIMEOUT` / `WRITE_TIMEOUT`). The server's own frontend-reply timeout
  is also 5 s (surfaced in-band as `ERR_REPLY_TIMEOUT`).

## Envelope rules (client-enforced)

Every response MUST satisfy, in validation order:

1. Valid UTF-8, else `ClientError::InvalidUtf8`.
2. Valid JSON, else `ClientError::InvalidJson`.
3. `jsonrpc` present and exactly `"2.0"`, else
   `InvalidEnvelope(WrongVersion)` (a missing member is the same violation).
4. `id` present as an unsigned integer, else `InvalidEnvelope(MissingId)`.
5. `id` equal to the request id, else `MismatchedResponseId {expected, actual}`.
6. Exactly one of `result` / `error`, else
   `InvalidEnvelope(BothResultAndError)` or
   `InvalidEnvelope(NeitherResultNorError)`.
7. If `error` is present it MUST be `{code: int, message: string, data?}`,
   else `InvalidEnvelope(InvalidErrorObject)`. A valid error object becomes
   `ClientError::Remote(RpcError)` — the in-band application-error path.

**Detecting "no app running":** `ClientError::is_not_running()` is the one
blessed check (true only for `SocketUnavailable`, covering both a missing
socket file and a stale socket refusing connections). Matching on message
text is forbidden.

**Server edge case:** if the server cannot parse a request at all, it replies
`ERR_PARSE` with `id: 0` (the wire type has no null id). A conforming client
never triggers this path — it serializes its own requests — and if such a
reply were cross-delivered the client would type it as `MismatchedResponseId`.

## Methods

All params/result shapes are the serde types in `scrybe-rpc/src/lib.rs`.
"App errors" lists the application-range codes a method can return in-band;
every method can additionally return `ERR_INVALID_PARAMS` (malformed params),
`ERR_METHOD_NOT_FOUND` (unknown method), `ERR_INTERNAL` (server-side emit or
registry failure), and — for the reply-correlated methods, which is all of
them except `close`/`quit` — `ERR_REPLY_TIMEOUT`.

| Method | Params | Result | App errors (beyond the common set) |
|---|---|---|---|
| `open` | `{path}` | `{tab_id, reloaded}` | — |
| `save` | `{path}` | `{path, bytes, was_dirty}` | `ERR_TAB_NOT_OPEN` |
| `read` | `{path}` | `{path, content, is_dirty}` | `ERR_TAB_NOT_OPEN` |
| `find` | `{pattern, paths?, literal?, case_sensitive?}` | `{hits: [{path, line, column, text}]}` | — |
| `section` | `{path, heading}` | `{heading, level, content}` | `ERR_TAB_NOT_OPEN`, `ERR_SECTION_NOT_FOUND` |
| `edit` | `{path, start_line, end_line, content}` | `{applied, size_after, is_dirty}` | `ERR_TAB_NOT_OPEN` |
| `list_tabs` | `{}` | `{tabs: [{path, title, is_dirty, view_mode, active}]}` | — |
| `reload` | `{path, force?}` | `{path, bytes, was_dirty}` | `ERR_TAB_NOT_OPEN`, `ERR_DIRTY_RELOAD_REFUSED` |
| `close` | `{path}` | `{applied}` (fire-and-forget ack; not-open collapses to `applied: false` frontend-side) | — |
| `quit` | `{force?}` | `{applied}` (fire-and-forget ack) | `ERR_DIRTY_QUIT_REFUSED` |
| `state` | `{}` | `{active_path, active_title, is_dirty, view_mode, theme, vim, wrap, open_paths}` | — |
| `set_theme` | `{theme}` | `{theme}` (applied) | — |
| `view_mode` | `{mode}` (`both`\|`edit`\|`preview`\|`cycle`) | `{mode}` (the CONCRETE mode now active) | — |
| `set_vim` | `{enabled}` | `{enabled}` (applied) | — |
| `logs` | `{tail?}` | `{lines}` (newest-last, from the app's in-memory ring) | — |

Notes:

- `find.paths` empty/omitted = search all open tabs; named paths fall back to
  disk when not open. `section.heading` is a case-insensitive substring match.
- `edit` writes the in-memory buffer only (tab goes dirty); `save` is the
  explicit persist.
- `save` against a not-open path errors `ERR_TAB_NOT_OPEN` on the wire; the
  CLI *presents* that as its documented silent no-op.

## Stable error codes

Standard JSON-RPC codes (outside the application range):

| Code | Name | Meaning |
|---|---|---|
| `-32700` | `ERR_PARSE` | Request line was not valid JSON. |
| `-32600` | `ERR_INVALID_REQUEST` | Request violated the envelope. |
| `-32601` | `ERR_METHOD_NOT_FOUND` | Unknown method. |
| `-32602` | `ERR_INVALID_PARAMS` | Params failed to deserialize / canonicalize. |
| `-32603` | `ERR_INTERNAL` | Server-side dispatch failure (emit failed, registry unavailable, malformed frontend reply). |

Application codes — reserved range `-32099..=-32000`
(`scrybe_rpc::APP_ERR_RANGE`), registry `scrybe_rpc::APP_ERROR_CODES`,
uniqueness + range enforced by test:

| Code | Name | Meaning |
|---|---|---|
| `-32001` | `ERR_TAB_NOT_OPEN` | The path is not an open tab (`read`/`edit`/`save`/`section`/`reload`; `close` collapses it to `applied: false`). |
| `-32002` | `ERR_DIRTY_QUIT_REFUSED` | `quit` with `force=false` while unsaved tabs exist. |
| `-32003` | `ERR_REPLY_TIMEOUT` | Frontend did not reply within 5 s (busy or modal-blocked). Retryable. |
| `-32004` | `ERR_SECTION_NOT_FOUND` | `section` heading matched nothing. |
| `-32005` | `ERR_DIRTY_RELOAD_REFUSED` | `reload` with `force=false` on a dirty tab. |

Once shipped in a release, a code keeps its number and meaning forever. New
codes take the next free number in the range, are appended to
`APP_ERROR_CODES`, and are added to this table.

## Malformed-reply examples → typed outcome

What the client guarantees when the server (or an imposter) misbehaves.
Each of these is pinned by a test in `scrybe-rpc/tests/wire_contract.rs`.

1. **Garbage bytes**
   ```
   {this is not json
   ```
   → `ClientError::InvalidJson(_)`

2. **Wrong echoed id** (request id was `1`)
   ```json
   {"jsonrpc":"2.0","id":42,"result":{}}
   ```
   → `ClientError::MismatchedResponseId { expected: 1, actual: 42 }`

3. **Both `result` and `error`**
   ```json
   {"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-32000,"message":"x"}}
   ```
   → `ClientError::InvalidEnvelope(EnvelopeError::BothResultAndError)`

4. **Missing / wrong `jsonrpc`**
   ```json
   {"id":1,"result":{}}
   ```
   → `ClientError::InvalidEnvelope(EnvelopeError::WrongVersion)`
   (same for `"jsonrpc":"1.0"`)

Also pinned: neither `result` nor `error` → `NeitherResultNorError`;
non-object `error` member → `InvalidErrorObject`; connection closed mid-frame
or with no reply → `Io(UnexpectedEof)`; silence past 5 s → `ReadTimeout`;
frame over 16 MiB → `FrameTooLarge`; missing socket file → `SocketUnavailable
{kind: NotFound}`; stale socket file → `SocketUnavailable {kind:
ConnectionRefused}` (both with `is_not_running() == true`); a valid in-band
error object → `Remote(RpcError)` with `code`/`message` intact.

## Failure taxonomy above the client (scrybe-tools)

For tool surfaces built on this contract (`scrybe-tools`, consumed by the CLI
and the MCP server):

| `ClientError` | `TransportError` | Surfaced as |
|---|---|---|
| `is_not_running() == true` | `NoApp` | business `tool_error` `no_live_app` |
| `Remote(RpcError)` | `Remote(RpcError)` | business `tool_error` `app_error` (the app answered) |
| everything else (I/O, timeouts, frame, UTF-8, JSON, envelope, mismatched id) | `Transport(String)` | `EngineFault::Transport` (the app did NOT answer) |

At the MCP boundary (since A4) *all three* rows surface as a tool result with
`isError: true` — `structuredContent.code` distinguishes them (`no_live_app` /
`app_error` / `transport`). The domain distinction still matters: a business
`tool_error` is a `ToolOutcome` (the tool ran), an `EngineFault` is not. On
the CLI, engine faults exit non-zero. The MCP mapping table is normative in
`scrybe-mcp-server/src/server.rs`; the frozen surface is
`docs/mcp-contract-0.6.json`.
