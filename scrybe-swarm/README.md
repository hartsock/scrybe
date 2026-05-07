<!--
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-swarm

NATS-based swarm chat sidebar and activity feed for collaborative editing.
Python on the outside, Rust on the inside.

## What it does

Connects to a NATS server and provides two live streams scoped to the
`ContentId` of the active document:

- **SwarmChat** — conversational messages between agents and the user, on
  subject `scrybe.chat.<doc_cid>`.
- **ActivityFeed** — structured events (agent name, action, doc CID) signalling
  what agents are doing, on subject `scrybe.activity.<doc_cid>`.

By scoping messages to the document's content identifier, multiple agents
working on different documents never see each other's traffic.

## Role in the architecture

`scrybe-swarm` is consumed by the Tauri backend to populate the swarm sidebar
panel in the UI. It depends on `scrybe-core` for `ContentId` only; it has no
dependency on the render or VCS crates. The NATS connection details are
configured by the operator (NATS server URL, credentials).

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `SwarmChat` | Publish/subscribe chat messages; `subject()` returns the NATS subject |
| `ActivityFeed` | Subscribe to activity events; `subject()` returns the NATS subject |
| `ActivityEvent` | `agent: String`, `action: String`, `doc_cid: String` — serde-serializable |

## Build and test

```sh
cargo build -p scrybe-swarm
cargo test -p scrybe-swarm
```

Requires a reachable NATS server for integration tests. Unit tests that do not
establish a connection run without external services. The `async-nats` crate
(v0.38) is the transport layer; Tokio is the async runtime.
