<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Design: Scrybe Plugin for ChatGPT and Codex

**Status:** Direction approved; detailed design awaiting review.
**Date:** 2026-08-22
**Builds on:** `vision-conversational-editing.md`, `mcp-rebuild.md`,
`cli-rpc.md`, and the platform-local RPC contract.

> Scrybe is not a chat window attached to a Markdown editor. It is a shared
> document workspace in which a human and an agent take turns proposing,
> reviewing, and committing changes.

## 1. Product boundary

The plugin is an access surface for Scrybe's existing Centaur/Cyborg authorship
model. It is not a second editor and does not introduce a separate document
state.

The Scrybe desktop app remains the visual source of truth. The plugin gives
ChatGPT and Codex:

1. Scrybe's existing MCP tools;
2. workflow guidance for safe human-agent authorship; and
3. a bootstrap path when the desktop app is absent, stopped, or incompatible.

The first release is local-first. A public hosted MCP service is deliberately
deferred because it would add accounts, synchronization, hosting, and a second
authority boundary before the local collaboration loop is validated.

This structure follows OpenAI's universal plugin shape: an installable package
may combine skills and an MCP server, while individual capabilities can remain
surface-specific. The package uses a local marketplace during development and
can later be submitted to the shared ChatGPT/Codex directory.

## 2. Invariants

The implementation must preserve these properties:

- **One editor, one buffer.** When a Scrybe tab is open, all reads and edits go
  through the live RPC path. The plugin must not substitute disk bytes for a
  dirty live buffer.
- **No silent fallback.** Missing or incompatible live collaboration returns a
  typed setup state. It never silently degrades to raw filesystem editing while
  claiming to operate on the shared document.
- **Explicit commitment.** Agent edits may make the live buffer dirty. Saving to
  disk remains an explicit command and the authorship skill asks before save
  unless the user already requested persistence.
- **Explicit acceptance before mutation.** An agent must stage a patch against
  the live buffer and present it for review before it can change that buffer.
  Only a distinct, user-approved acceptance action may apply the patch; a
  request to edit, an approval to save, or a model tool call is not acceptance.
  Acceptance is bound to the revision read when the patch was prepared, so a
  human edit made during review invalidates the staged patch rather than being
  overwritten.
- **Trusted installation.** Detection may inspect the host, but installation
  only uses a hard-coded official Scrybe release origin, an immutable version
  and digest, a release manifest signature verified against a public key pinned
  in the host, and an explicit approval capability. A checksum fetched from the
  release channel alone is transport-integrity evidence, not release
  authenticity. After the host begins at that official origin, redirect
  destinations are untrusted delivery transports: only the signed artifact
  identity and content address authorize execution.
- **Plain-text sovereignty.** The plugin does not upload documents to a Scrybe
  service. Documents stay in the user's workspace and remain ordinary files.
- **One tool engine.** Plugin tools are backed by `scrybe-tools`; schemas and
  behavior are not copied into a JavaScript adapter.

## 3. Package shape

The repository gains a plugin package and one Rust binary:

```text
plugins/scrybe/
|-- .codex-plugin/plugin.json
|-- .mcp.json
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

`plugin.json` identifies Scrybe and points at the bundled skill and MCP
configuration. `.mcp.json` launches `scrybe-plugin-host`. The host links the
existing `scrybe-mcp-server` and `scrybe-tools` libraries in-process, then adds
the setup surface described below and an acceptance-gated authorship adapter.
The adapter may expose proposal and acceptance capabilities, but it must not
give an agent an unchecked path to the live-buffer mutation operation.

The install-facing copy is:

- Display name: **Scrybe**
- Short description: **Centaur-style collaborative document authorship**
- Category: **Productivity**
- Capabilities: **Read**, **Write**

"Markdown editor with AI" remains useful shorthand, but it is not the product
definition used by the skill or long description.

## 4. Runtime states

The host always starts, even when the Scrybe GUI is not installed. Discovery
produces one of these typed states:

| State | Meaning | Behavior |
|---|---|---|
| `live_compatible` | Compatible app is running and the RPC probe succeeds. | Serve the complete live tool surface. |
| `installed_stopped` | Compatible app is installed but no live endpoint answers. | Serve headless tools; live tools return `no_live_app` and offer launch. |
| `installed_incompatible` | App is present but its RPC contract is incompatible. | Refuse live operations and offer the pinned compatible update. |
| `not_installed` | No trusted installation is found. | Serve headless tools plus setup tools and offer installation. |
| `discovery_failed` | Host inspection failed or produced contradictory evidence. | Return the evidence and refuse installation or live claims. |

Discovery evidence is ordered by strength:

1. a successful live RPC capability probe returning the app version and RPC
   contract version;
2. a platform-native installed-application record at a trusted absolute path;
3. a known Scrybe installation location with version metadata; and
4. a PATH candidate, reported as untrusted until its path and version are
   validated.

The adapter never executes an arbitrary PATH result for installation or update.

## 5. Setup MCP surface

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

Mutating and approval-gated. Accepts only a valid unexpired `plan_id`; it does
not accept a URL, command, version, path, or digest from the model. It downloads
to a private temporary file, verifies the digest, and launches the native
installer visibly. It never uses a shell-interpolated command.

The tool returns after launch with a typed state. A later status probe confirms
installation and compatibility. Cancellation, digest mismatch, elevation
denial, and installer failure remain distinct outcomes.

### `scrybe_setup_launch`

Mutating and approval-gated. Starts an already-installed app only from the
trusted absolute path returned by discovery. It accepts no executable path or
arguments from the model. The result includes the post-launch RPC capability
probe or a typed launch failure.

Starting an app is separate from installing it. No environment variable silently
opts every future session into app launch.

## 6. Platform installation policy

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

## 7. Centaur-authorship skill

The bundled skill teaches the model to collaborate through the document rather
than answer beside it. Its default loop is:

1. call setup status and establish whether a compatible live app exists;
2. open the requested document in Scrybe;
3. read the live buffer and record its content/revision identity;
4. resolve the requested section or object;
5. prepare the smallest reviewable patch, bound to the read revision;
6. present the patch and wait for explicit user acceptance;
7. apply the accepted patch only if its precondition still matches the live
   buffer, otherwise re-read and prepare a new patch;
8. render or lint when the accepted change affects presentation or structure;
9. summarize what changed and leave the buffer dirty for human review; and
10. save only when the user explicitly approves or originally requested save.

The skill treats a stale precondition as collaboration, not an error to erase:
it re-reads, explains the conflict, and re-bases the proposed edit. It never
overwrites a newer human revision merely to complete its own turn.

Headless render, lint, and provenance tools remain useful when the GUI is absent.
The skill labels those operations as headless and does not describe them as a
shared live editing session.

## 8. Distribution

Development uses a repository marketplace entry at
`.agents/plugins/marketplace.json` and the package at `plugins/scrybe/`. The
plugin is installed and tested in a new ChatGPT/Codex task after each cache
refresh.

Release packaging publishes the native `scrybe-plugin-host` beside Scrybe's
existing platform binaries. The plugin package resolves the host for the current
OS/architecture using the same no-lifecycle-script, optional-platform-package
pattern already used by `@scrybe-ai/cli`.

Plugin and Scrybe versions move in lockstep. Compatibility is still negotiated
from the RPC contract version rather than guessed from display-version equality.

The app RPC surface gains a backward-compatible `probe` method returning the app
version, RPC contract version, platform, and architecture. Connection alone is
only liveness evidence; a peer that cannot answer this probe is incompatible for
the plugin's live path.

Public directory submission is a later release step. It requires successful
local marketplace evidence on supported ChatGPT and Codex surfaces and does not
require a hosted copy of the user's documents.

## 9. Error and security model

- Setup results distinguish missing software, stopped app, incompatible
  protocol, unavailable release metadata, digest mismatch, user cancellation,
  elevation denial, installer failure, and post-install probe failure.
- User or model strings never become executable names, command lines, download
  origins, or installer arguments.
- A checksum manifest fetched from the release channel is never treated as an
  independent trust root. The host verifies the manifest signature against its
  bundled pinned public key before accepting its asset or digest.
- Redirects are permitted only after the initial request to the hard-coded
  official release endpoint. Their destinations are untrusted transports, not
  authorities: HTTPS is required; credentials and non-standard ports are
  rejected; redirects are bounded; and each resolved and connected address is
  checked to reject loopback, private, link-local, and other reserved networks.
  The connection check prevents DNS rebinding from bypassing that rule.
- Release metadata and the downloaded asset are size-bounded. The downloaded
  asset is never launched until its size, signed identity, and SHA-256 content
  address all verify.
- Temporary files use restrictive permissions and are removed after success or
  failure.
- Logs redact user paths where possible and never contain document contents,
  credentials, or approval capabilities.
- A `plan_id` is single-use, expires quickly, and is invalidated if release
  metadata changes.
- Automated tests never install software, launch a real installer, or call live
  GitHub/OpenAI services.

## 10. Test strategy

### Deterministic automated tests

- Discovery table tests for every runtime state and evidence ordering.
- Trusted-path validation, PATH substitution, symlink/reparse escape, and
  version/contract mismatch tests.
- Mocked release metadata and asset server tests for origin validation,
  untrusted redirect handling, redirect and size bounds, private-address and
  DNS-rebinding rejection, signature and digest verification, malformed or
  unsigned metadata, wrong-key signatures, and network failure.
- Approval-capability tests for expiry, replay, mutation, and cross-plan use.
- Staged-patch tests proving an agent cannot mutate the live buffer before an
  explicit, revision-bound human acceptance; cover rejection, expiry, replay,
  stale human edits during review, and the fact that save approval does not
  authorize patch application.
- Installer-launch abstraction tests proving exact executable and argument
  propagation without running an installer.
- MCP contract tests for setup tools and parity tests proving core Scrybe tool
  schemas still come from `scrybe-tools`.
- RPC capability-probe tests covering compatible, old/method-not-found,
  malformed, and impostor endpoints.
- Skill lint and scenario tests for live-buffer preference, dirty-buffer safety,
  stale human edit handling, and explicit save.
- Plugin manifest, marketplace, packaging, and platform-binary resolution tests.

### Manual Windows release evidence

1. Plugin starts with no Scrybe GUI installation discoverable and reports
   `not_installed`.
2. A deliberately fake `scrybe` earlier on PATH is reported but never executed.
3. The adapter presents the exact official installer version, URL, and digest.
4. Installation requires a visible approval and installer interaction.
5. Post-install status identifies the trusted installed app.
6. Launch establishes the named-pipe live session.
7. A ChatGPT/Codex task opens, reads, edits, renders, and saves one small document
   while the GUI displays the same buffer.
8. Before save, the agent sees its live edit while disk bytes remain unchanged;
   after save, disk bytes and reported content identity agree.

The manual installer test is never part of unattended CI.

## 11. Delivery sequence

1. Add the plugin host with discovery and read-only setup status.
2. Package the plugin manifest, MCP configuration, marketplace entry, and
   Centaur-authorship skill.
3. Add immutable install planning and fully mocked verification tests.
4. Add the approval-gated Windows installer launcher and native smoke evidence.
5. Run the complete repository gate and a real local ChatGPT/Codex live-buffer
   workflow.
6. Open a `risk:high` PR because the change adds executable distribution and an
   installer path. Human approval is required before merge.

## 12. Non-goals

- A second embedded Markdown editor inside ChatGPT or Codex.
- Hosted document storage or synchronization.
- Automatic installation, update, launch, or save without explicit approval.
- Silent raw-filesystem fallback for a requested live collaboration session.
- Model-provider configuration inside Scrybe.
- Multi-human real-time editing.
- Public plugin-directory submission in the first PR.
