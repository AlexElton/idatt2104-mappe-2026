# RGA Collaborative Editor

[![CI](https://github.com/AlexElton/idatt2104-mappe-2026/actions/workflows/ci.yml/badge.svg)](https://github.com/AlexElton/idatt2104-mappe-2026/actions)

## Introduction

A browser-based collaborative text editor backed by a **Replicated Growable Array (RGA)** CRDT implemented from scratch in Rust.

Traditional collaborative editors use Operational Transformation (OT), which requires a central server to serialize and transform concurrent operations. RGA instead uses *tombstone-based conflict-free replication*: each character carries a globally unique **S4Vector** identifier `⟨ssn, sid, sum, seq⟩` (Definition 9 in Roh et al. 2011). Concurrent inserts at the same position are resolved deterministically by comparing S4Vectors — no locking, no transformation functions, no central arbiter.

**Properties guaranteed by the RGA algorithm:**
- **Operation Commutativity (OC):** applying any two ops in either order produces the same result.
- **Precedence Transitivity (PT):** the insertion ordering is globally consistent across all sites.

Together these guarantee **eventual consistency** — after all pending ops are delivered, every site converges to the same document.

## Implemented Functionality

- **RGA Insert** (Algorithm 8) — concurrent inserts at the same position resolved by S4Vector ordering.
- **RGA Delete** (Algorithm 9) — tombstone-based deletion; deleted nodes remain in the linked list so future ops can still reference them by `s_k`.
- **RGA Update** (Algorithm 10) — character replacement with Delete-wins precedence.
- **WebSocket server** — `axum` + `tokio` server assigning each client a unique `site_id`; per-client RGA instances ensure each site's S4Vectors carry the correct `sid`.
- **Late-joiner hydration** — new clients have their RGA hydrated by replaying the existing document's ops, so they can delete and update pre-existing content.
- **Sync interval slider** — models a partitioned / slow network. Higher interval = longer effective disconnect between syncs; convergence is demonstrated when both clients finally sync.
- **Multi-client** — N concurrent browser tabs, each independently tracked.

## Future Work / Known Weaknesses

- **Tombstone accumulation** — deleted nodes are never purged. Document memory grows monotonically. Section 5.6 of the paper describes a purging protocol but it is not implemented.
- **No persistent history** — restarting the server clears the document.
- **Single-server only** — the server is the communication hub. True peer-to-peer distribution is not supported.
- **No undo/redo** — explicitly out of scope for this assignment.

## External Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `tokio` | 1 | Async runtime; task spawning, channels, TCP listener |
| `axum` | 0.7 | HTTP router and WebSocket upgrade handler (`ws` feature) |
| `serde` | 1 | Derive macros for serialization (`#[derive(Serialize, Deserialize)]`) |
| `serde_json` | 1 | JSON encoding/decoding of WebSocket messages |
| `futures-util` | 0.3 | `SinkExt` / `StreamExt` for async WebSocket read/write |

No external CRDT libraries are used. All RGA logic is hand-implemented from Roh et al. 2011.

## Installation

Requires Rust (stable, 2024 edition). Install via [rustup](https://rustup.rs/).

```bash
git clone https://github.com/AlexElton/idatt2104-mappe-2026.git
cd idatt2104-mappe-2026
cargo build --release
```

## Usage

```bash
cargo run --release
```

Open `http://localhost:3000` in two or more browser tabs.

**Demonstrating convergence with a simulated network partition:**
1. Open two tabs.
2. Set one tab's sync interval slider to 3000 ms.
3. Type in both tabs simultaneously.
4. Observe that the high-interval tab lags behind.
5. After 3 seconds both tabs display the same merged text — this is the CRDT convergence property in action.

## Running Tests

```bash
cargo test
```

Unit tests are in `src/crdt/rga.rs` (module `tests`). They verify:
- Single and sequential inserts
- Concurrent inserts at the same position (dOPT puzzle — Example 1 from the paper)
- Delete followed by a concurrent insert (Example 2 from the paper)
- Convergence when ops are applied in opposite orders (OC property)
- Delete-wins over a concurrent Update (Algorithm 10)

## API Documentation

```bash
cargo doc --open
```

## Attribution

All RGA algorithms are taken directly from:

> Roh, H. G., Jeon, M., Kim, J. S., & Lee, J. (2011).
> *Replicated abstract data types: Building blocks for collaborative applications.*
> Journal of Parallel and Distributed Computing, 71(3), 354–368.

Specific mappings:

| Function | Paper reference |
|---|---|
| `S4Vector::precedes()` | Definition 9 |
| `Rga::findlist()` | Algorithm 4 |
| `Rga::remote_insert()` | Algorithm 8 |
| `Rga::remote_delete()` | Algorithm 9 |
| `Rga::remote_update()` | Algorithm 10 |
| SVI hash table (node lookup by `s_k`) | Section 5.4 |
