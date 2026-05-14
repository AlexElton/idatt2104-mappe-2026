# Design Spec — RGA Collaborative Text Editor

**Date:** 2026-05-14
**Project:** IDATT2104 Network Programming Assignment
**Deadline:** 2026-05-26
**Academic source:** Roh et al. 2011 — "Replicated abstract data types: Building blocks for collaborative applications"

---

## 1. Goal

Implement a browser-based collaborative text editor backed by a Rust WebSocket server running an RGA (Replicated Growable Array) CRDT. No external CRDT libraries. The demo must show concurrent edits from multiple browser tabs merging correctly. Grade target: A/B.

---

## 2. Architecture

```
Browser Tab A (site_id=1)        Browser Tab B (site_id=2)
  <textarea>                        <textarea>
  input → ops queue                 input → ops queue
  setInterval → send batch          setInterval → send batch
        │                                 │
        └──────────── WebSocket ──────────┘
                          │
              Rust server (axum + tokio)
                          │
         ┌────────────────┴──────────────────┐
         │         AppState (Arc)             │
         │  clients: RwLock<HashMap>          │
         │    1 → ClientInfo{rga(sid=1), ...} │
         │    2 → ClientInfo{rga(sid=2), ...} │
         └───────────────────────────────────┘
```

Each connected client has its own **RGA instance on the server** with its own `site_id`. This is what gives each site's insertions a unique `sid` in their S4Vectors, making concurrent insert resolution deterministic via Definition 9 (Roh et al. 2011).

---

## 3. Server State

The current `Registry` abstraction is extended to carry per-client RGA state:

```rust
struct ClientInfo {
    tx: mpsc::UnboundedSender<ServerMsg>,
    rga: Rga,             // site_id = this client's assigned ID
    pending_ops: Vec<Op>, // remote ops queued until this client's next sync
}

// AppState replaces the split Registry/AppState from the scaffold
struct AppState {
    clients: Arc<RwLock<HashMap<u64, ClientInfo>>>,
    next_site_id: Arc<AtomicU64>,
}
```

---

## 4. Sync Processing Algorithm

Called each time a client X sends an ops batch. This is the core CRDT moment.

```
receive [{op:"insert"|"delete", pos, char?}, ...] from client X:

Step 1 — local ops (must run before applying remote ops):
  for each incoming op:
    clients[X].rga.local_insert(pos, char) or local_delete(pos)
      → generates Op::Insert{left: S4Vector, obj, s_k} with X's sid
    push that S4Vector Op into every OTHER client's pending_ops

Step 2 — drain X's pending remote ops:
  for each op in clients[X].pending_ops:
    clients[X].rga.remote_insert/delete/update(...)

Step 3 — broadcast:
  canonical = clients[X].rga.to_string()
  client_count = clients.len()
  send {"type":"state","text":canonical,"clients":client_count} to ALL clients (incl. X)
  clear clients[X].pending_ops
```

**Why Step 1 before Step 2:** X's ops carry positions relative to X's current textarea state. X's RGA must still reflect that view when converting positions to S4Vectors. After Step 1 converts positions to stable S4Vector-based ops, Step 2 applies remote ops — the CRDT handles ordering via Algorithm 8's scan. All RGAs converge to the same canonical text regardless of processing order (OC property, Roh et al. 2011).

---

## 5. Wire Protocol

All messages are JSON over WebSocket.

### Client → Server

```json
{"type": "ops", "ops": [
  {"op": "insert", "pos": 3, "char": "a"},
  {"op": "delete", "pos": 2}
]}
```

Sent on each sync interval. Position is 0-indexed into the client's current visible text. Empty ops array is valid (no-op ping).

### Server → Client

```json
{"type": "state", "text": "world hello", "clients": 2}
```

Sent after every sync from any client, to **all** clients including the sender. The sender's textarea is updated to canonical too (handles the case where X's local view diverged). `clients` is the current connected count, used for the status badge.

```json
{"type": "init", "site_id": 2, "text": "current canonical text"}
```

Sent once on connect. Client uses `site_id` only for display (the server handles all CRDT logic).

---

## 6. RGA Core — Required Implementations

All in `src/crdt/`. Academic source for each:

| Function | Algorithm | Notes |
|---|---|---|
| `S4Vector::precedes(&self, other)` | Definition 9 | `sa ≺ sb` iff ssn/sum/sid comparisons |
| `Rga::findlist(pos)` | Algorithm 4 | Walk linked list, skip tombstones, count |
| `Rga::local_insert(pos, char)` | — | findlist → get left S4V → Op::Insert |
| `Rga::local_delete(pos)` | — | findlist → get target S4V → Op::Delete |
| `Rga::local_update(pos, char)` | — | findlist → get target S4V → Op::Update |
| `Rga::remote_insert(left, obj, s_k)` | Algorithm 8 | Scan right from left; skip nodes whose s_k succeeds ours |
| `Rga::remote_delete(target, s_o)` | Algorithm 9 | Tombstone node, update s_p |
| `Rga::remote_update(target, obj, s_o)` | Algorithm 10 | Check s_o vs s_p precedence; skip if Delete won |
| `Rga::to_string()` | — | Walk head via link, collect non-tombstone obj |
| `Rga::apply(op)` | — | Already implemented; dispatches to remote_* |

---

## 7. Server — Required Implementations

| Location | What |
|---|---|
| `server/registry.rs::broadcast()` | Iterate HashMap, skip sender, `tx.send(msg.clone())` |
| `server/handler.rs::handle_socket` send task | Loop: recv from `rx`, serialize, `sink.send(WsMessage::Text)` |
| `server/handler.rs::handle_socket` recv task | Loop: `stream.next()`, deserialize, call sync handler |
| New: sync handler function | Implements Section 4 algorithm above |
| `AppState` | Add `clients: Arc<RwLock<HashMap<u64, ClientInfo>>>` alongside or replacing `Registry` |

The existing `serve_index`, `ws_handler`, `router`, and `AppState::assign_site_id` stay unchanged.

---

## 8. Frontend — `static/index.html`

Single self-contained HTML file. No framework, no build step.

**Layout:**
```
┌─────────────────────────────────────────────────┐
│ RGA Collaborative Editor                         │
├─────────────────────────────────────────────────┤
│                                                  │
│  [           <textarea>  (fills width)         ] │
│                                                  │
│  🟢 Site #2 · 2 clients · next sync: 0.6s       │
│                                                  │
│  Sync interval: |────●──────| 1000ms            │
│                  100ms      5000ms               │
└─────────────────────────────────────────────────┘
```

**Status badge states:**
- `🟢 Connected` — WebSocket open, sync running
- `🟡 Syncing...` — actively sending/receiving
- `🔴 Disconnected` — WebSocket closed

**JS responsibilities:**
1. Connect WebSocket to `ws://[host]/ws`
2. On `init`: store `site_id`, set textarea to initial text
3. On `input` event: compare old vs new text at cursor → push `{op, pos, char?}` to queue
4. `setInterval(syncFn, intervalMs)`: send `{type:"ops", ops:[...]}`, clear queue
5. On `state` message: update textarea value if text differs (preserve cursor when possible)
6. Slider `input` event: update `intervalMs`, restart interval

**Sync interval = delay simulation:** High interval models a slow/partitioned network. Client is effectively "offline" between syncs. This is what the demo shows — two clients with different intervals will see divergence and then convergence.

---

## 9. Tests

### Unit tests (in `src/crdt/rga.rs`)

| Test | What it verifies | Paper reference |
|---|---|---|
| `test_single_insert` | Basic insert + to_string | — |
| `test_concurrent_insert_same_position` | dOPT puzzle: 3 concurrent inserts at same pos converge | Example 1 |
| `test_delete_then_concurrent_insert` | Insert at tombstone position works correctly | Example 2 |
| `test_convergence_two_orderings` | Apply ops A→B and B→A, assert same result | OC property |
| `test_update_loses_to_delete` | Concurrent Delete + Update: Delete wins | Algorithm 10 |

### Integration tests (in `tests/convergence.rs`, optional)

Spawn server in background, connect N tokio WebSocket clients, fire concurrent ops, assert all final `to_string()` values are equal.

---

## 10. README Structure

Required by assignment. Key academic content:

- **Introduction:** Define CRDT vs OT. Explain why RGA was chosen (Insert+Delete+Update, tombstone-based, no transformation functions needed). Cite Roh et al. 2011.
- **External dependencies:**
  - `tokio` — async runtime for WebSocket server
  - `axum` — HTTP routing and WebSocket upgrade handler
  - `serde` + `serde_json` — op serialization over WebSocket
  - `futures-util` — async stream/sink combinators for WebSocket handling
- **Attribution:** Roh et al. 2011 is the primary source for S4Vector (Definition 9), SVI hash table scheme (Section 5.4), and all three remote algorithms (Algorithms 8, 9, 10). Every non-trivial function in `src/crdt/` cites the corresponding algorithm.
- **Future work / weaknesses:** Tombstone purging not implemented (Section 5.6); document grows unbounded. No reconnect state recovery. Single-server deployment only (not truly distributed). No undo.

---

## 11. Out of Scope

- Tombstone purging — future work
- Undo / redo
- Persistence across server restarts
- Authentication
- JS-side RGA (server is canonical store)
- OT-based conflict resolution (RGA makes this unnecessary)
