# experimental/

Crates that are not part of the shipping workspace.  Ideas are preserved here;
none of this code is compiled by default.

| Crate | What it was | Why parked |
|---|---|---|
| `scrybe-swarm` | NATS swarm-chat sidebar + activity feed | Requires external NATS infrastructure; 67 LOC; not shipping in v0.5 |
| `scrybe-panels` | Bake-off orchestrator + SQLite calibration log | Depends on multi-agent panel infrastructure; 78 LOC; not shipping in v0.5 |

These may be revived as standalone crates or folded into `scrybe-core` when
the scope justifies it.  Until then they live here, uncompiled.
