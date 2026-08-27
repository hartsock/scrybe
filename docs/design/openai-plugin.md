<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Design: Scrybe Plugin for ChatGPT and Codex

**Status:** Direction approved; concurrency and authority revision awaiting review.
**Date:** 2026-08-22
**Builds on:** `vision-conversational-editing.md`, `mcp-rebuild.md`,
`cli-rpc.md`, and the platform-local RPC contract.

> Scrybe is not a chat window attached to a Markdown editor. It is a shared
> document workspace in which a human and an agent take turns proposing,
> reviewing, and committing changes.

## 1. Product boundary

The plugin is an access surface for Scrybe's existing Centaur/Cyborg authorship
model. It is not a second editor and does not introduce a separate authoritative
document state. A staged proposal may retain an immutable base snapshot and a
candidate result for review, but the Scrybe live buffer remains the only mutable
document authority.

The Scrybe desktop app remains the visual source of truth. The package gives
supported ChatGPT and Codex surfaces:

1. a deliberately projected subset of Scrybe's tool engine;
2. workflow guidance for safe human-agent authorship; and
3. a bootstrap path when the desktop app is absent, stopped, or incompatible.

The first release is local-first. A public hosted MCP service is deliberately
deferred because it would add accounts, synchronization, hosting, and a second
authority boundary before the local collaboration loop is validated.

This structure follows OpenAI's universal plugin shape: an installable package
may combine skills and an MCP server, while individual capabilities can remain
surface-specific. The transports are not interchangeable. Codex can launch a
bundled local stdio MCP server. ChatGPT connects only to a remote HTTPS MCP
endpoint; private or developer-machine access therefore requires Secure MCP
Tunnel. The first implementation is Codex-local. ChatGPT support follows only
after its tunnel bootstrap, plan availability, and write-action behavior have
native evidence.

## 2. Invariants

The implementation must preserve these properties:

- **One editor, one buffer.** When a Scrybe tab is open, all reads and edits go
  through the live RPC path. The plugin must not substitute disk bytes for a
  dirty live buffer.
- **No lost updates.** Every human or agent mutation advances a monotonic buffer
  revision. A BLAKE3 content digest identifies the resulting bytes and may
  repeat after an ABA change. An agent proposal is bound to the exact
  `{buffer_id, base_revision, base_content_digest}` it read. Applying against
  any other state requires an explicit rebase; a stale patch never overwrites
  newer human or agent work.
- **Editor-owned serialization.** Scrybe owns one proposal train per buffer and
  performs the compare-and-swap at the live-buffer boundary. MCP hosts and
  agents cannot race by maintaining private last-writer-wins copies.
- **Human activity gates automation.** While the editor reports an active human
  input transaction or short human-edit lease, delegated agent proposals may be
  staged and merged but are held before live-buffer application. A human may
  explicitly review and apply a held proposal in Scrybe.
- **No silent fallback.** Missing or incompatible live collaboration returns a
  typed setup state. It never silently degrades to raw filesystem editing while
  claiming to operate on the shared document.
- **Structural authority.** Raw mutation tools are absent from the plugin-facing
  registry. A model may stage a proposal or request review, but only Scrybe can
  apply it under a human action or an editor-held delegated capability. The
  capability is never returned to the model.
- **Separate persistence.** Applying an accepted proposal may make the live
  buffer dirty. Saving to disk is a separate human action or single-use
  persistence authority bound to the exact revision and content identity. Edit
  acceptance never implies save acceptance.
- **Trusted installation.** Detection may inspect the host, but installation
  only uses a hard-coded official Scrybe release origin, signed non-rollbackable
  metadata, an immutable version and digest, and host-owned human consent.
- **Plain-text sovereignty.** Scrybe stores ordinary workspace files and does
  not upload them to a Scrybe service. Content deliberately supplied to a model
  still travels through that model provider's inference path; local storage and
  inference disclosure are distinct claims.
- **One tool engine, projected capabilities.** Plugin tools reuse
  `scrybe-tools` schemas and handlers where the authority matches. A
  plugin-facing registry projection hides unsafe handlers and adds the
  proposal/authority adapters; schemas and behavior are not copied into a
  JavaScript shim.
- **No ambient-authority overclaim.** A registry projection cannot prevent a
  Codex task from using separately granted shell or filesystem tools. Scrybe
  reports conflict-safe `brokered` mode only when the harness supplies a real
  witness that direct writes to shared documents are denied. Without that
  witness, the session is labeled `cooperative`, and external writes are
  detected and held but not claimed impossible.

## 3. Package shape

The repository gains a plugin package and one Rust binary:

```text
plugins/scrybe/
|-- .codex-plugin/plugin.json
|-- .mcp.json                 # Codex: bundled local stdio host
|-- .app.json                 # ChatGPT: added only with a registered tunnel endpoint
|-- skills/
|   `-- centaur-authorship/
|       `-- SKILL.md
`-- assets/

scrybe-plugin-host/
|-- Cargo.toml
`-- src/
    |-- main.rs
    |-- discovery.rs
    |-- install.rs
    `-- server.rs
```

`plugin.json` identifies Scrybe and points at the bundled skill and the
capabilities supported on the current product surface. For Codex, `.mcp.json`
launches `scrybe-plugin-host`. The host links `scrybe-tools` in-process, builds
the plugin-facing registry projection, and adds the setup and authorship
surfaces described below. It does not expose the normal MCP server wholesale.

ChatGPT does not execute `.mcp.json`. Its later `.app.json` maps to a registered
remote MCP connection whose Secure MCP Tunnel terminates at the same local
plugin host. Until that bridge exists, the universal listing advertises the
Codex-local capability only and does not claim local ChatGPT editing or setup.

The install-facing copy is:

- Display name: **Scrybe**
- Short description: **Centaur-style collaborative document authorship**
- Category: **Productivity**
- Capabilities: **Read**, **Write**

"Markdown editor with AI" remains useful shorthand, but it is not the product
definition used by the skill or long description.

## 4. Plugin-facing capability projection

The normal CLI and general-purpose `scrybe-mcp-server` remain backward
compatible. The plugin host constructs a different registry for model-facing
authorship. This is an authority boundary, not a presentation filter.

The initial projection contains:

| Class | Plugin behavior |
|---|---|
| Live reads (`read`, `section`, scoped `list_tabs`, selected `state`) | Reuse the shared handler and schema, extended with buffer identity and revision. |
| Pure analysis (`render`, `lint`) | Reuse the shared handler and schema. |
| Scoped search | Wrap `find` so it searches only authorized live buffers or an explicitly authorized headless workspace and never silently falls back elsewhere on disk. |
| Safe open | Wrap `open` so paths are canonical and session-authorized and opening an existing dirty tab never reloads or replaces its buffer. |
| Authorship | Expose proposal tools only; raw `edit` is absent. |
| Persistence | Expose a save request; raw `save` is absent. |
| Destructive live operations | Raw `reload`, `close_tab`, and `quit`, including force variants, are absent. |
| Other file writes | `embed`, `export`, `export_figures`, and `mermaid_to_png` are absent until a separate output-authority design exists. |
| Unneeded UI mutation and diagnostics | `set_theme`, `set_vim`, `view_mode`, and `logs` are absent from the first authorship release. |

Every discovery and dispatch route is generated from this projected registry.
If progressive discovery later adds `tool_search`, `tool_describe`, or
`tool_invoke`, those operations receive the projection, never the underlying
default registry. Calling a hidden tool by name returns `unknown_tool`; there
is no privileged escape hatch that accepts an arbitrary registered tool name.

The plugin-specific authorship tools are:

- `scrybe_proposal_stage`: stage a patch against an exact buffer revision;
- `scrybe_proposal_status`: return state, queue position, and blocking reason;
- `scrybe_proposal_list`: list proposal metadata for the current buffer;
- `scrybe_proposal_diff`: return the reviewable candidate diff;
- `scrybe_proposal_cancel`: cancel the caller's unapplied proposal;
- `scrybe_proposal_request_review`: bring the trusted Scrybe review UI forward;
  and
- `scrybe_save_request`: ask Scrybe to present persistence approval for an
  exact revision.

None accepts `approved=true`, an executable action, or a capability supplied by
the model.

This projection governs calls that enter through the Scrybe plugin. It does not
magically revoke tools granted elsewhere by the agent harness. The setup status
therefore reports `document_write_path: brokered | cooperative`:

- `brokered` requires an enforcement witness from the host sandbox or OCAP
  layer that direct writes to the shared document are unavailable and the
  projected Scrybe route is the only mutation path; and
- `cooperative` means the merge train protects Scrybe proposals, while the
  filesystem watcher and save conflict gate detect ambient external writes.

The plugin never fabricates a brokered witness from its own registry contents.
Compatible confinement may be supplied by agent-bridle or another harness, but
the first implementation does not make agent-bridle a mandatory Scrybe runtime
dependency.

## 5. Buffer identity and proposal authority

### Buffer revision

Each open tab has an editor-generated opaque `buffer_id`, a monotonic `revision`
counter, a BLAKE3 `content_digest`, and the `disk_content_digest` last observed
when loading or saving. Every human input transaction, accepted agent proposal,
reload, undo, redo, or other content mutation increments the revision,
including an ABA change that returns to identical bytes. The content digest
records the live bytes; the revision records history and ordering; the disk
digest detects an external persistence race.

Live `read` returns all three values. An authoring proposal contains at least:

```text
proposal_id
actor_id                 # host-issued; not model-selected
buffer_id
base_revision
base_content_digest
base_snapshot_ref        # immutable, editor-private
candidate_snapshot_ref   # immutable, editor-private
patch_digest
created_at
state
```

`scrybe-core::ContentDigest` supplies the existing bare BLAKE3 byte identity; it
is not a CID and does not itself store anything. This work introduces a local
immutable proposal store whose snapshot bytes are keyed by that digest and
whose canonical-CBOR lineage records have their own digest. The full base and
candidate text need not be repeated through the model transcript. Proposal
lineage records the originating actor, base record, candidate record, rebase
records, acceptance event, and resulting buffer record without overwriting
earlier records. The implementation must not call a bare digest a stored record
or an IPFS/IPLD CID.

### Session and buffer scope

An MCP connection begins with no live-document authority. Scrybe grants it an
explicit set of shared `buffer_id` values or canonical workspace roots. Existing
tabs outside that set do not appear in `list_tabs`, search, state, or proposal
results. Safe open canonicalizes the target, rejects symlink/reparse escape from
the authorized root, and asks in the trusted Scrybe UI before adding an
out-of-scope path. Agents participating in one collaborative session may share
proposal metadata only for buffers the human granted to that session.

### Authority modes

Scrybe supports two explicit modes per buffer:

1. **Suggestion mode (default).** The model can stage and inspect proposals.
   The Scrybe UI shows the diff, and a human Accept or Reject action is the only
   way to apply or reject it.
2. **Delegated co-author mode.** A human uses Scrybe to grant one plugin session
   a time-bounded, buffer-scoped authority for specified patch operations.
   Scrybe or the plugin host retains that authority out of band; it is never
   placed in MCP output, model context, logs, or tool arguments. Delegation may
   auto-apply only a clean, current-revision proposal while no human-edit lease
   is active.

Changing buffers, sessions, operation scope, expiry, or authority mode requires
a new human action. Revocation is immediate. Neither mode grants persistence.

Human acceptance is bound to
`{session, buffer_id, current_revision, patch_digest}` and is single-use. A
rebase creates a new patch digest and invalidates prior acceptance. A model tool
call, MCP approval configuration, or a boolean in tool input is never evidence
of human acceptance.

Manual Scrybe Save remains directly available to the human. A model-originated
save request is bound to
`{buffer_id, revision, content_digest, disk_content_digest}`. Scrybe re-reads the
disk identity before writing. If either live or disk state changed before
redemption, the request becomes stale and enters conflict review instead of
saving newer content under an older approval. In `brokered` mode the harness
witness closes the direct-write race; in `cooperative` mode Scrybe reports that
the final filesystem compare is detection, not exclusive mediation.

## 6. Per-buffer merge train

Scrybe owns a serialized proposal train for each live buffer. Agents do not
apply private copies or independently retry a stale edit.

1. Staging records the immutable base and candidate, assigns a sequence, and
   returns `staged` or `queued`; it never reports `applied` prematurely.
2. At the head of the train, Scrybe compares the proposal base with the current
   revision and content identity.
3. An exact match becomes `ready`. Suggestion mode waits for the Scrybe UI;
   delegated mode may proceed only when its authority and human-activity gates
   are satisfied.
4. A mismatch invokes a deterministic three-way merge over
   `base / current / proposed` using a proven merge implementation rather than
   hand-written line heuristics.
5. A clean merge creates a new content-addressed candidate, revision binding,
   and patch digest. It returns `rebased` for review; previous acceptance is
   invalid. A delegated session may continue only if its scope explicitly
   permits clean rebases.
6. An overlapping or ambiguous merge becomes `conflicted`. Conflict markers are
   never written into the live buffer. The proposal moves to a held lane so it
   does not indefinitely block unrelated later work; the human or originating
   agent can inspect the three sides and stage a new resolution against the
   then-current revision.
7. Final application runs on the editor's serialized event loop and performs a
   compare-and-swap immediately before mutation. If that check fails because a
   human or another proposal changed the buffer, the proposal returns to merge
   evaluation. It never uses unconditional retry or last-writer-wins.

The observable states are `staged`, `queued`, `held_human_active`, `ready`,
`rebased`, `conflicted`, `accepted`, `applied`, `rejected`, `cancelled`, and
`superseded`. Every nonterminal tool result includes the current revision,
queue position, and a typed reason. Agents must use status rather than infer
success from transport completion.

Human input opens an editor-owned activity lease for the active input
transaction and 1,500 ms after its latest local edit, measured with a monotonic
clock. IME composition holds the lease until composition ends. Proposals may be
computed and staged during the lease, but delegated application returns
`held_human_active`. Because final application and human mutation are serialized
and revision-checked, an edit at either edge of the lease still cannot be lost.

Multiple plugin hosts identify their sessions independently but submit to the
same app-owned train. A proposal from agent B therefore observes an application
by agent A as a revision change and must merge, hold, or conflict. Scrybe emits
proposal-state events; transports without event delivery use status polling.
An external disk change updates `disk_content_digest`; a clean buffer may load
it as a new revision, while a dirty buffer holds it as an external side of the
same three-way conflict workflow.

## 7. Runtime states

The Codex-local host always starts, even when the Scrybe GUI is not installed.
A ChatGPT connection reaches these states only after the operator has installed
and registered Secure MCP Tunnel plus the local bridge; ChatGPT cannot bootstrap
an absent local MCP process through a tool call. Discovery produces one of these
typed states:

| State | Meaning | Behavior |
|---|---|---|
| `live_compatible` | Compatible app is running and the RPC probe succeeds. | Serve the complete live tool surface. |
| `installed_stopped` | Compatible app is installed but no live endpoint answers. | Serve headless tools; live tools return `no_live_app` and offer launch. |
| `installed_incompatible` | App is present but its RPC contract is incompatible. | Refuse live operations and offer the pinned compatible update. |
| `not_installed` | No trusted installation is found. | Serve headless tools plus setup tools and offer installation. |
| `discovery_failed` | Host inspection failed or produced contradictory evidence. | Return the evidence and refuse installation or live claims. |

Discovery evidence is ordered by strength, but compatibility and identity are
reported separately:

1. a platform-native installed-application record at a trusted absolute path,
   with expected publisher or package evidence;
2. a known Scrybe installation location whose executable and version metadata
   agree with that installation record;
3. an authenticated live RPC capability probe returning the app version, RPC
   contract version, process identity, and a challenge response tied to the
   trusted installation; and
4. a PATH candidate, reported as untrusted evidence only and never executed by
   setup or launch.

A peer that merely speaks the RPC protocol proves compatibility, not identity.
An unauthenticated probe can report `compatible_untrusted`, but it cannot cause
installation, launch, acceptance, persistence, or delegated-authority actions.

The adapter never executes an arbitrary PATH result for installation or update.

## 8. Setup MCP surface

The plugin host adds four namespaced tools without changing the frozen core MCP
contract:

### `scrybe_setup_status`

Read-only. Returns the typed runtime state, installed and required versions,
live endpoint evidence, platform, architecture, and available next actions. It
does not launch, download, or modify anything.

### `scrybe_setup_plan`

Read-only. Resolves an install or update from the official
`hartsock/scrybe` release channel and returns:

- exact version;
- exact asset name and HTTPS URL;
- expected SHA-256 digest;
- signed release-manifest identity, content address, size bound, and verified
  key identifier;
- publisher/repository identity;
- whether elevation or visible installer UI is expected; and
- a short-lived opaque `plan_id` bound to those exact values.

The release workflow publishes a machine-readable checksum manifest alongside
the assets and signs it with the release key. The host verifies that signature
against its pinned public key before it trusts the manifest's asset name,
version, platform, architecture, size, and SHA-256 digest. A release without a
valid signature and digest is not installable through the plugin. Redirects
after the initial hard-coded official release endpoint are untrusted transports,
not trust decisions: the downloaded bytes must still match the signed artifact
identity and content address before they may be launched. The tool returns
instructions rather than weakening verification.

### `scrybe_setup_install`

Mutating and host-consent-gated. On Codex, accepts only a valid unexpired
`plan_id`; it does not accept a URL, command, version, path, or digest from the
model. Invocation presents a trusted native consent surface containing the
resolved version, publisher, origin, digest, and elevation expectation. A human
action in that surface creates a single-use install authority retained by the
host; MCP approval or the initiating tool call does not. The host then downloads
to a private temporary file, verifies the signed release metadata and asset
digest, and launches the native installer visibly. It never uses a
shell-interpolated command.

The tool returns after launch with a typed state. A later status probe confirms
installation and compatibility. Cancellation, digest mismatch, elevation
denial, and installer failure remain distinct outcomes.

### `scrybe_setup_launch`

Mutating and host-consent-gated unless the human has enabled launch for this
plugin session. Starts an already-installed app only from the trusted absolute
path returned by discovery. It accepts no executable path or arguments from the
model. The result includes the post-launch authenticated RPC capability probe or
a typed launch failure.

Starting an app is separate from installing it. No environment variable silently
opts every future session into app launch.

### ChatGPT bootstrap boundary

These setup tools cannot install the first local ChatGPT bridge because no MCP
connection exists yet. The ChatGPT listing therefore gives an operator-owned,
signed native bootstrap procedure for the bridge and Secure MCP Tunnel. Once
that connection is established, the same setup tools may inspect, install, and
launch the Scrybe GUI. The first release does not claim ChatGPT support until
that procedure has end-to-end evidence on an eligible ChatGPT plan and surface.

## 9. Platform installation policy

Initial implementation and release evidence are Windows-first because this
design is being exercised on a real Windows host.

### Windows

- Detect the installed Tauri app from trusted installation records and known
  per-user or machine paths.
- Resolve the matching official Scrybe release asset.
- Verify its published SHA-256 digest before execution.
- Launch `Scrybe_<version>_x64-setup.exe` with visible UI and no silent flags.
- Re-probe the named-pipe endpoint and installed version after completion.

### macOS and Linux

The same state and plan schemas are implemented, but an install action is
exposed only after that platform has native packaging tests. Until then the
result is `operator_action_required` with an official release link and digest.
This is an honest bounded capability, not a claimed install success.

## 10. Centaur-authorship skill

The bundled skill teaches the model to collaborate through the document rather
than answer beside it. Its default loop is:

1. call setup status and establish whether a compatible live app exists;
2. open the requested document in Scrybe;
3. read the live buffer and record its buffer, revision, and content identity;
4. resolve the requested section or object;
5. stage the smallest reviewable proposal against that exact base;
6. report whether it is ready, queued, held for human activity, rebased, or
   conflicted instead of claiming that transport success means application;
7. inspect a rebased or conflicted result and stage a new resolution when
   appropriate, without overwriting the current buffer;
8. read back the live buffer after an applied result;
9. render or lint when the change affects presentation or structure;
10. summarize the proposal/application state and leave an applied buffer dirty
    for human review; and
11. request persistence only when the user explicitly asked for it, accepting
    that Scrybe may require a separate human action.

The skill treats a stale precondition as collaboration, not an error to erase.
It does not independently rewrite a patch onto fresh bytes. Scrybe performs the
deterministic rebase, preserves the three inputs, and returns a typed result. On
conflict, the agent explains what moved and proposes a resolution against the
new revision. It never overwrites newer human or agent work merely to complete
its own turn.

When several agents participate, each uses its own `actor_id` and proposal
status. The skill names queueing and conflicts plainly; it does not conceal a
held proposal as a completed edit. An agent may inspect other proposal metadata
for the same user-authorized buffer, but proposal contents are disclosed only
within that collaborative session.

Headless render, lint, and provenance tools remain useful when the GUI is absent.
The skill labels those operations as headless and does not describe them as a
shared live editing session.

## 11. Surface transports and distribution

Development uses a repository marketplace entry at
`.agents/plugins/marketplace.json` and the package at `plugins/scrybe/`.
Codex-local installation and testing use the bundled `.mcp.json` stdio host.

Release packaging publishes the native `scrybe-plugin-host` beside Scrybe's
existing platform binaries. The plugin package resolves the host for the current
OS/architecture using the same no-lifecycle-script, optional-platform-package
pattern already used by `@scrybe-ai/cli`.

Plugin and Scrybe release versions may move together, but their wire contracts
do not assume lockstep installation. The plugin-facing tool schema and RPC
protocol are independently versioned and support a declared compatibility
window. Compatibility is negotiated from protocol versions rather than guessed
from display-version equality.

The app RPC surface gains a backward-compatible `probe` method returning the app
version, RPC contract version, platform, architecture, server process evidence,
and an authenticated challenge response. Connection alone is only liveness;
speaking the protocol is compatibility; the trusted installation plus challenge
establishes the endpoint used for authority-bearing operations.

For ChatGPT, the package later adds `.app.json` for a registered HTTPS MCP
connection. ChatGPT uses a frozen, administrator-approved snapshot of tool
definitions, so additive optional fields remain backward compatible and
breaking schema changes require a new protocol version plus an administrator
refresh. Secure MCP Tunnel carries the connection to the local bridge; the
plugin package does not convert local stdio into a ChatGPT endpoint.

Public directory submission is a later release step. It requires successful
evidence on each advertised surface and does not require a hosted copy of the
user's documents. A shared directory listing may advertise different
capabilities by surface.

## 12. Error, trust, and data-flow model

### Threat boundary

The first release treats model-originated tool calls, prompt injection,
concurrent agents, stale edits, other OS users, PATH substitution, endpoint
mix-ups, network tampering, and release rollback as in scope. A process that has
already compromised the current user's OS account or administrator/root is out
of scope; per-user IPC permissions cannot honestly defend against that actor.
The implementation still minimizes same-user ambient authority and does not use
an unauthenticated endpoint as an installation or approval root.

### Local IPC and authority

- Unix creates the socket beneath a `0700` per-user directory, uses restrictive
  socket permissions, and verifies peer UID where the platform supports it.
- Windows creates the named pipe with an explicit DACL for the current user SID
  and required system principals. The client obtains the server process ID and
  verifies its trusted installation path and publisher evidence before any
  authority-bearing operation.
- An installation-bound challenge authenticates the Scrybe session after the
  platform check. Authentication material is kept out of model-visible tool
  input and output.
- `SCRYBE_SOCK` remains a developer override for ordinary CLI use but is not
  trusted by the plugin's installer, launcher, proposal-acceptance, delegation,
  or persistence paths unless an operator explicitly configures a trusted
  absolute development endpoint.
- A failed identity, challenge, revision, authority, or activity-lease check is
  fail-closed. The host never falls back to raw filesystem editing or the normal
  unrestricted MCP registry.

### Release trust and setup

- Setup results distinguish missing software, stopped app, untrusted endpoint,
  incompatible protocol, unavailable or expired release metadata, signature or
  digest mismatch, rollback, user cancellation, elevation denial, installer
  failure, and post-install probe failure.
- User or model strings never become executable names, command lines, download
  origins, installer arguments, manifest locations, or signing keys.
- A long-lived offline root key delegates replaceable release-signing keys. A
  signed manifest includes repository identity, release version, monotonically
  increasing sequence, publication and expiry times, asset names, sizes, and
  SHA-256 digests. The plugin packages a minimum accepted sequence and stores
  the highest accepted sequence to reject rollback.
- Root rotation requires metadata signed by the currently trusted root and the
  replacement root. Revocation removes a release key in newer root metadata;
  an expired or revoked key cannot authorize installation. Recovery that cannot
  satisfy this chain is an operator action, not an automatic downgrade.
- Redirects are allowed only after the initial request to the hard-coded
  official release endpoint. Each destination is untrusted transport input:
  HTTPS is required; credentials and non-standard ports are rejected; redirect
  count is bounded; and every resolved and connected address is checked to
  reject loopback, private, link-local, and other reserved networks. Checking
  both resolution and the connected peer prevents DNS rebinding. Redirects
  never replace the signed artifact identity, size, and digest checks.
- Release metadata and assets are size-bounded. Temporary files use restrictive
  permissions and are removed after success or failure.
- A `plan_id` is single-use, expires quickly, and is invalidated if release
  metadata, platform, architecture, or selected asset changes.

### Document data flow

| Boundary | Document content behavior |
|---|---|
| Workspace and Scrybe | Ordinary files plus content-addressed local proposal/provenance records; no Scrybe-hosted document copy. |
| Local app RPC | Selected live-buffer content, patches, diffs, and metadata stay on the machine and use authenticated per-user IPC. |
| Codex or ChatGPT inference | Content and tool results selected for the task enter that product/model interaction under its configured data controls. |
| Secure MCP Tunnel | Carries MCP traffic over its configured secure channel between ChatGPT and the local bridge; it does not create Scrybe document storage. |
| Release setup | Exchanges release metadata and installer bytes only; it never transmits document content. |

Logs redact user paths where possible and never contain document contents,
credentials, proposal snapshots, or authority material. Automated tests never
install software, launch a real installer, or call live GitHub/OpenAI services.

## 13. Test strategy

### Deterministic automated tests

- Plugin-projection tests proving raw `edit`, `save`, forced `reload`,
  `close_tab`, `quit`, and other file writers are absent from listing,
  discovery, description, and direct invocation while safe shared schemas still
  come from `scrybe-tools`.
- Discovery table tests for every runtime state, identity/compatibility split,
  and evidence ordering.
- Trusted-path validation, PATH substitution, symlink/reparse escape, and
  version/contract mismatch tests.
- Mocked release metadata and asset server tests for origin validation,
  redirects, size bounds, private/reserved-address and DNS-rebinding rejection,
  signature and digest verification, key rotation, revocation, expiry, rollback,
  malformed metadata, and network failure.
- Proposal-authority tests proving model input cannot manufacture acceptance,
  delegated authority is buffer/session/scope/expiry bound, revocation is
  immediate, and no authority material appears in MCP results or logs.
- Revision tests for every human and agent mutation, including ABA content;
  stale compare-and-swap; and content-addressed proposal lineage.
- Deterministic merge-train tests for two agents from the same base,
  non-overlapping clean rebase, overlapping conflict, held-lane progress,
  cancellation, ordering, rebase invalidating acceptance, and no conflict
  markers entering the live buffer.
- Human-concurrency tests using a virtual monotonic clock for input transaction,
  IME composition, lease edges, an edit between review and application, and an
  edit during final compare-and-swap.
- Persistence tests proving accepted edit does not imply save, stale save
  approval cannot persist a newer live or disk revision, and manual Save remains
  available.
- Authority-report tests proving a projected registry alone remains
  `cooperative`, a valid harness witness admits `brokered`, and a missing or
  weak witness cannot be promoted.
- Installer-launch abstraction tests proving exact executable and argument
  propagation without running an installer.
- RPC capability-probe tests covering compatible, old/method-not-found,
  malformed, wrong-user, wrong-process, failed-challenge, and impostor
  endpoints, including explicit Windows named-pipe DACL assertions.
- Skill lint and scenario tests for live-buffer preference, dirty-buffer safety,
  queue/status honesty, stale human and agent edits, conflict handling, and
  explicit save.
- Plugin manifest, marketplace, packaging, platform-binary resolution, and
  surface split tests proving `.mcp.json` is the Codex local path and ChatGPT is
  not advertised without a registered tunnel app.

### Manual Windows release evidence

1. Plugin starts with no Scrybe GUI installation discoverable and reports
   `not_installed`.
2. A deliberately fake `scrybe` earlier on PATH is reported but never executed.
3. The adapter presents the exact official installer version, URL, and digest.
4. Installation requires a visible approval and installer interaction.
5. Post-install status identifies the trusted installed app.
6. Launch establishes an authenticated current-user named-pipe live session and
   rejects a deliberately impersonating endpoint.
7. A Codex task opens and reads one small document while the GUI displays the
   same buffer and identity.
8. Human typing after proposal staging advances the revision and holds or
   rebases the proposal without losing either change.
9. Two independent plugin-host sessions stage non-overlapping and overlapping
   edits; the clean case advances through the train and the conflict case is
   held with all three inputs intact.
10. Suggestion-mode Accept applies the reviewed patch. A delegated session can
    apply a clean current patch while idle, but is held during human activity.
11. Before Save, the agent sees the accepted live edit while disk bytes remain
    unchanged; after a separate human Save, disk bytes, revision, and content
    identity agree.
12. A direct external write is detected and held in `cooperative` mode; the same
    route is unavailable under a real `brokered` write-denial witness.

The manual installer test is never part of unattended CI. ChatGPT gets a
separate native evidence checklist after Secure MCP Tunnel bootstrap exists; a
Codex result is not relabeled as ChatGPT evidence.

## 14. Delivery sequence

This design PR remains the reviewable contract. Implementation is split into
focused, stacked changes so concurrency, authority, installation, and transport
do not hide one another's risk:

1. Add buffer identity, monotonic revisions, content-addressed proposal records,
   the app-owned merge train, trusted review UI, and deterministic concurrency
   tests.
2. Add the Codex-local plugin host, capability projection, read-only discovery,
   authorship tools, manifest, marketplace entry, and Centaur-authorship skill.
3. Add signed immutable install planning, fully mocked verification, the
   host-consent-gated Windows installer launcher, and native Windows evidence.
4. Run the complete repository gate and a real Codex live-buffer workflow with
   simultaneous human and two-agent edits.
5. Design and implement the ChatGPT Secure MCP Tunnel bootstrap as a separate
   surface-specific change, then run its own eligible-plan write evidence.
6. Keep executable-distribution and installer PRs `risk:high`; human approval is
   required before merge.

## 15. Non-goals

- A second embedded Markdown editor inside ChatGPT or Codex.
- Hosted document storage or synchronization.
- Automatic installation, update, launch, or save without explicit approval.
- Silent raw-filesystem fallback for a requested live collaboration session.
- Model-provider configuration inside Scrybe.
- Multi-human real-time editing.
- Last-writer-wins agent edits, silent automatic conflict resolution, or writing
  conflict markers into the authoritative live buffer.
- Treating MCP approval configuration as proof that a human accepted a patch.
- Claiming local ChatGPT support through a Codex `.mcp.json` process.
- Claiming that a plugin registry alone confines ambient Codex shell or
  filesystem authority.
- Public plugin-directory submission in the first PR.
