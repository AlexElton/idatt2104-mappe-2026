# Development Plan — RGA Collaborative Editor

12 days · 3 students · Claude Max available · Deadline 26 May 2026

---

## Repo structure (set up Day 1, ~30 min with Claude)

```
src/
  main.rs              ← S3
  crdt/
    mod.rs
    s4vector.rs        ← S1
    node.rs            ← S1
    rga.rs             ← S1
    op.rs              ← agree Day 1, commit together
  server/
    mod.rs
    registry.rs        ← S2
    handler.rs         ← S2
static/
  index.html           ← Claude generates this
Cargo.toml
README.md              ← S3, written last
```

---

## Day 1 — Do together (2–3 hours)

Commit `op.rs` so everyone can work independently from Day 2:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct S4Vector { pub ssn: u64, pub sid: u64, pub sum: u64, pub seq: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Op {
    Insert { left: Option<S4Vector>, obj: char, s_k: S4Vector },
    Delete { target: S4Vector, s_k: S4Vector },
    Update { target: S4Vector, obj: char, s_k: S4Vector },
}

// WebSocket wire envelope — wraps an Op with the originating site's ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message { pub site_id: u64, pub op: Op }
```

Once this compiles and is pushed, all three are unblocked.

---

## Student 1 — RGA Core

**Files:** `s4vector.rs`, `node.rs`, `rga.rs`

### Week 1 (days 2–6): implement and unit test in isolation

| File | What to implement |
|------|------------------|
| `s4vector.rs` | `S4Vector::precedes(&self, other) -> bool` (Definition 9 from paper), `S4Vector::new(ssn, sid, sum, seq)` |
| `node.rs` | `struct Node { obj, s_k, s_p, link (index), next (index) }`, tombstone detection |
| `rga.rs` | `struct RGA { nodes: Vec<Node>, hash: HashMap<S4Vector, usize>, head: Option<usize> }` |
| | `local_insert(pos: usize, obj: char) -> Op` |
| | `local_delete(pos: usize) -> Op` |
| | `local_update(pos: usize, obj: char) -> Op` |
| | `remote_insert(op)` — Algorithm 8 from paper |
| | `remote_delete(op)` — Algorithm 9 |
| | `remote_update(op)` — Algorithm 10 |
| | `to_string() -> String` — walk linked list, skip tombstones |

### Tests to write (use Claude for boilerplate, write assertions yourself)

- `test_concurrent_insert_same_position` — dOPT puzzle, Example 1 from paper
- `test_delete_then_insert_at_tombstone` — Example 2 from paper
- `test_convergence_two_sites` — apply ops in both orders, assert same result
- `test_update_loses_to_delete`

### Week 2 (days 7–8)
Tombstone purging (Section 5.6 of paper), then help S3 debug integration failures.

---

## Student 2 — WebSocket Server

**Files:** `registry.rs`, `handler.rs`

### Cargo.toml dependencies to add

```toml
tokio = { version = "1", features = ["full"] }
axum = { version = "0.7", features = ["ws"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
futures-util = "0.3"
```

### Week 1 (days 2–6)

**`registry.rs`**
```rust
struct Registry {
    clients: Arc<RwLock<HashMap<u64, mpsc::Sender<Message>>>>
}
fn connect(site_id) -> Receiver
fn broadcast(from_site: u64, msg: &Message)
fn disconnect(site_id)
```

**`handler.rs`**
- `GET /` → serve `static/index.html`
- `GET /ws` → WebSocket upgrade
- On connect: assign `site_id`, send it to client, register in Registry
- On message: deserialize `Op`, wrap in `Message`, broadcast to all others
- On disconnect: remove from Registry

### Days 4–5 target
Get two `wscat` connections exchanging mock ops. RGA does not need to be working yet.

### Week 2 (days 7–8)
Integrate S1's RGA (server keeps its own RGA instance), help debug race conditions.

---

## Student 3 — Integration, Tests, Glue, README

**Files:** `main.rs`, `tests/convergence.rs`, `README.md`

### Days 2–4: test harness (main Week 1 deliverable)

```rust
// tests/convergence.rs
// Spawn server in background, connect N tokio WebSocket clients,
// fire concurrent ops, collect final to_string() from each, assert equal.

#[tokio::test]
async fn test_two_clients_converge() {
    let addr = spawn_test_server().await;
    let (mut c1, _) = connect(&addr).await;
    let (mut c2, _) = connect(&addr).await;

    // c1 inserts "hello", c2 inserts "world" concurrently
    // wait for both to receive each other's ops
    // assert c1.rga.to_string() == c2.rga.to_string()
}
```

### Days 5–6
`main.rs` — wire server, start listening, handle Ctrl-C gracefully.

### Days 7–8
Run convergence tests against real server + RGA. Find and report bugs to S1/S2.

### Days 9–10
README (required sections listed below) + frontend with Claude.

### Days 11–12
Presentation prep, final polish.

---

## Frontend — Claude task, Day 9 (half a day)

Ask Claude to generate `static/index.html` with:
- WebSocket connection to `ws://localhost:3000/ws`
- `<textarea>` synced to received RGA state
- Delay slider (0–3000ms) — buffers incoming ops with `setTimeout`
- Status badge: 🟢 Live / 🟡 Delayed / 🔴 Offline

Review it, verify the op JSON matches your wire format, done.

---

## Timeline

| Day | S1 | S2 | S3 |
|-----|----|----|-----|
| 1 | **Together:** repo setup, `op.rs`, Cargo.toml | | |
| 2 | `s4vector.rs` + tests | `registry.rs` skeleton | test harness skeleton |
| 3 | `node.rs` | `handler.rs`, HTTP serving | test harness: connect + send |
| 4 | `rga.rs` local ops | WebSocket upgrade, broadcast | test harness: concurrent ops |
| 5 | `rga.rs` remote insert | mock op round-trip working | convergence assertions |
| 6 | remote delete + update, unit tests pass | server stable with two clients | `main.rs` wiring |
| 7 | **Integration day** — plug RGA into server, first real test | | |
| 8 | Fix bugs from convergence tests | | |
| 9 | Tombstone purging | Polish | README draft + frontend |
| 10 | | | Frontend done, README complete |
| 11 | **Together:** demo run-through, edge case fixes | | |
| 12 | **Submit** | | |

---

## Where to use Claude Max vs write yourself

| Use Claude for | Write yourself |
|---|---|
| Cargo.toml dependency versions | RGA algorithms (Algorithms 8–10 from paper) |
| Tokio/axum boilerplate | `S4Vector::precedes()` ordering logic |
| Test boilerplate (`#[tokio::test]`, setup/teardown) | Convergence test assertions |
| serde derives and JSON shapes | The remote Insert scanning loop |
| Frontend HTML/JS entirely | README content — you know the system |
| README formatting and structure | README "future work" — you understand the tradeoffs |

**Rule of thumb:** use Claude for anything you would copy-paste from docs anyway. Write yourself anything that touches the algorithm.

---

## README sections required by assignment

- [ ] Solution name + CI link
- [ ] Introduction
- [ ] Implemented functionality
- [ ] Future work / known weaknesses
- [ ] External dependencies (each with description and purpose)
- [ ] Installation instructions
- [ ] Usage instructions
- [ ] How to run tests
- [ ] API documentation link (if applicable)
- [ ] Attribution of all external sources and code
