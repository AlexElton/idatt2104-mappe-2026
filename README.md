# Collaborative Editor

## Installation

Requires Rust (stable, 2024 edition), pnpm, wasm-pack, and bacon. Install Rust via [rustup](https://rustup.rs/).

```bash
git clone https://github.com/AlexElton/idatt2104-mappe-2026.git
cd idatt2104-mappe-2026
cargo install wasm-pack --locked
cargo install bacon --locked
pnpm install
```

## Usage

Start the backend and React/Vite frontend:

```bash
pnpm dev
```

Nx runs the backend and web dev targets in parallel. The backend target uses `bacon backend`, while the frontend keeps Vite's WebAssembly HMR plugin for `crates/core`.

You can also run either side directly:

```bash
pnpm dev:backend
pnpm dev:web
```

## Production Builds

```bash
pnpm build:web
pnpm build:backend
```

`pnpm build:web` runs the Nx `rga-core:build-release` target before building the SPA into `apps/web/dist/`. `pnpm build:backend` runs the Nx backend release target.

## Introduction

A browser-based collaborative text editor backed by a **Replicated Growable Array (RGA)** CRDT implemented from scratch in Rust.

Traditional collaborative editors use Operational Transformation (OT), which requires a central server to serialize and transform concurrent operations. RGA instead uses _tombstone-based conflict-free replication_: each character carries a globally unique **S4Vector** identifier `⟨ssn, sid, sum, seq⟩` (Definition 9 in Roh et al. 2011). Concurrent inserts at the same position are resolved deterministically by comparing S4Vectors — no locking, no transformation functions, no central arbiter.

**Properties guaranteed by the RGA algorithm:**

- **Operation Commutativity (OC):** applying any two ops in either order produces the same result.
- **Precedence Transitivity (PT):** the insertion ordering is globally consistent across all sites.

Together these guarantee **eventual consistency** — after all pending ops are delivered, every site converges to the same document.

## Demo the app

Open the Vite URL printed by the web dev target in two or more browser tabs.

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

## Attribution

All RGA algorithms are taken directly from:

> Roh, H. G., Jeon, M., Kim, J. S., & Lee, J. (2011).
> _Replicated abstract data types: Building blocks for collaborative applications._
> Journal of Parallel and Distributed Computing, 71(3), 354–368.
