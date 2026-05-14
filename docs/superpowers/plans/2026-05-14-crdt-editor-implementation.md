# RGA Collaborative Editor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill in all `todo!()` stubs in the RGA CRDT engine and WebSocket server, then add a working frontend and README.

**Architecture:** Per-client RGA instances on the server (each site gets unique `sid`). Client sends position-based ops on a sync interval. Server runs the sync algorithm (local-ops-first, then drain pending remote ops), broadcasts canonical text to all clients.

**Tech Stack:** Rust 2024, tokio 1, axum 0.7 (ws feature), serde/serde_json, futures-util. No CRDT libraries.

**Spec:** `docs/superpowers/specs/2026-05-14-crdt-editor-design.md`

**Academic source (cite in code comments):** Roh et al. 2011 — "Replicated abstract data types: Building blocks for collaborative applications"

---

## File Map

| File | Status | What changes |
|---|---|---|
| `src/crdt/node.rs` | Modify | Change `obj: Option<char>` → `obj: char` + `tombstone: bool` |
| `src/crdt/s4vector.rs` | Modify | Implement `precedes()` |
| `src/crdt/rga.rs` | Modify | Implement all `todo!()` methods + tests |
| `src/server/registry.rs` | Rewrite | Add `ClientInfo{tx,rga,pending_ops}`, add `process_sync()` |
| `src/server/handler.rs` | Modify | Implement WS send/recv tasks, update `AppState` |
| `static/index.html` | Rewrite | Full textarea+JS client |
| `README.md` | Create | Full assignment README |

No changes to: `src/crdt/op.rs`, `src/crdt/mod.rs`, `src/server/mod.rs`, `src/main.rs`, `Cargo.toml`.

---

## Task 1 — Fix Node struct + implement S4Vector::precedes()

**Files:**
- Modify: `src/crdt/node.rs`
- Modify: `src/crdt/s4vector.rs`

The current `Node` stores `obj: Option<char>` and uses `None` for tombstones. This loses the original character, making RGA hydration for late-joining clients impossible and complicating debug output. Store `obj: char` always; add a separate `tombstone: bool` flag. Also implement `precedes()` per Definition 9 from the paper.

- [ ] **Step 1: Update node.rs**

Replace the entire file content:

```rust
use crate::crdt::s4vector::S4Vector;

/// A single node in the RGA linked list.
///
/// `obj`       — stored character; always present (tombstones keep their original char).
/// `tombstone` — true means this node has been deleted (visible only to Algorithm 8).
/// `s_k`       — immutable S4Vector set on Insert; used as SVI hash key and precedence key.
/// `s_p`       — mutable S4Vector; updated by Delete/Update for precedence (Algorithms 9 & 10).
/// `link`      — index of next node in linked list (None = tail).
/// `next`      — index of next node in SVI hash chain (separate chaining, unused for now).
#[derive(Debug, Clone)]
pub struct Node {
    pub obj:       char,
    pub tombstone: bool,
    pub s_k:       S4Vector,
    pub s_p:       S4Vector,
    pub link:      Option<usize>,
    pub next:      Option<usize>,
}

impl Node {
    pub fn new(obj: char, s_k: S4Vector) -> Self {
        Self {
            obj,
            tombstone: false,
            s_p: s_k.clone(),
            s_k,
            link: None,
            next: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}
```

- [ ] **Step 2: Implement S4Vector::precedes() in s4vector.rs**

Replace the `todo!()` in the `precedes` method:

```rust
/// Returns true if self has LOWER priority than other (Definition 9, Roh et al. 2011).
/// sa ≺ sb iff: sa.ssn < sb.ssn
///           OR sa.ssn == sb.ssn AND sa.sum < sb.sum
///           OR sa.ssn == sb.ssn AND sa.sum == sb.sum AND sa.sid < sb.sid
pub fn precedes(&self, other: &S4Vector) -> bool {
    if self.ssn != other.ssn {
        return self.ssn < other.ssn;
    }
    if self.sum != other.sum {
        return self.sum < other.sum;
    }
    self.sid < other.sid
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build 2>&1 | head -30
```

Expected: warnings about unused code, but zero errors. (The `node.rs` change will cause errors in `rga.rs` if any code references `obj: Option<char>` — but since all RGA methods are `todo!()`, there's nothing to break.)

- [ ] **Step 4: Commit**

```bash
git add src/crdt/node.rs src/crdt/s4vector.rs
git commit -m "feat(crdt): fix Node struct, implement S4Vector::precedes (Def 9)"
```

---

## Task 2 — Rga::remote_insert() + findlist() + to_string()

**Files:**
- Modify: `src/crdt/rga.rs`

`remote_insert` (Algorithm 8) is the most complex piece. `findlist` and `to_string` are needed to test it. Implement all three together and write tests that run without any local ops.

- [ ] **Step 1: Write the failing tests** (add to the `#[cfg(test)]` block at the bottom of rga.rs)

Replace the existing test stubs with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn s4v(ssn: u64, sid: u64, sum: u64) -> S4Vector {
        S4Vector::new(ssn, sid, sum, sum)
    }

    // -----------------------------------------------------------------------
    // Task 2 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_rga_is_empty_string() {
        let rga = Rga::new(1, 0);
        assert_eq!(rga.to_string(), "");
    }

    #[test]
    fn test_single_remote_insert_at_head() {
        let mut rga = Rga::new(1, 0);
        rga.remote_insert(None, 'a', s4v(0, 1, 1));
        assert_eq!(rga.to_string(), "a");
    }

    #[test]
    fn test_sequential_remote_inserts() {
        let mut rga = Rga::new(1, 0);
        let s_a = s4v(0, 1, 1);
        let s_b = s4v(0, 1, 2);
        rga.remote_insert(None, 'a', s_a.clone());
        rga.remote_insert(Some(s_a), 'b', s_b);
        assert_eq!(rga.to_string(), "ab");
    }

    /// dOPT puzzle — concurrent inserts at the same position converge.
    /// Site 1 (sid=1) inserts 'a', site 2 (sid=2) inserts 'b', both at head.
    /// Higher sid wins → 'b' (sid=2) appears before 'a' (sid=1).
    #[test]
    fn test_concurrent_insert_same_position() {
        let s_a = s4v(0, 1, 1); // lower priority
        let s_b = s4v(0, 2, 1); // higher priority (sid 2 > 1)

        // Order 1: insert 'a' first, then 'b'
        let mut rga1 = Rga::new(1, 0);
        rga1.remote_insert(None, 'a', s_a.clone());
        rga1.remote_insert(None, 'b', s_b.clone());

        // Order 2: insert 'b' first, then 'a'
        let mut rga2 = Rga::new(2, 0);
        rga2.remote_insert(None, 'b', s_b.clone());
        rga2.remote_insert(None, 'a', s_a.clone());

        assert_eq!(rga1.to_string(), rga2.to_string());
        assert_eq!(rga1.to_string(), "ba"); // b (sid=2) wins → appears first
    }

    // -----------------------------------------------------------------------
    // Task 3–5 tests — filled in later tasks
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_insert() {
        todo!()
    }

    #[test]
    fn test_concurrent_insert_same_position_local() {
        todo!()
    }

    #[test]
    fn test_delete_then_concurrent_insert() {
        todo!()
    }

    #[test]
    fn test_convergence_two_orderings() {
        todo!()
    }

    #[test]
    fn test_update_loses_to_delete() {
        todo!()
    }
}
```

- [ ] **Step 2: Run — verify the active tests compile but fail**

```bash
cargo test test_empty_rga_is_empty_string test_single_remote_insert_at_head test_sequential_remote_inserts test_concurrent_insert_same_position 2>&1 | tail -20
```

Expected: compilation errors about missing implementations.

- [ ] **Step 3: Implement findlist(), to_string(), and remote_insert() in rga.rs**

Replace the `findlist`, `to_string`, and `remote_insert` `todo!()`s:

```rust
/// Algorithm 4 (findlist): return the index of the pos-th visible node (1-indexed).
/// pos=0 → None (virtual head, used as left=None for insert-at-head).
fn findlist(&self, pos: usize) -> Option<usize> {
    if pos == 0 {
        return None;
    }
    let mut count = 0;
    let mut cur = self.head;
    while let Some(idx) = cur {
        if !self.nodes[idx].is_tombstone() {
            count += 1;
            if count == pos {
                return Some(idx);
            }
        }
        cur = self.nodes[idx].link;
    }
    None
}

/// Walk the linked list and collect all non-tombstone characters in order.
pub fn to_string(&self) -> String {
    let mut result = String::new();
    let mut cur = self.head;
    while let Some(idx) = cur {
        if !self.nodes[idx].is_tombstone() {
            result.push(self.nodes[idx].obj);
        }
        cur = self.nodes[idx].link;
    }
    result
}

/// Algorithm 8 (Roh et al. 2011).
/// Insert a new node after `left` (None = insert at head).
/// Scan rightward, skipping nodes whose s_k has HIGHER priority than s_k,
/// until we find one with lower priority — insert before it.
pub fn remote_insert(&mut self, left: Option<S4Vector>, obj: char, s_k: S4Vector) {
    // Create node and register in hash table
    let new_idx = self.nodes.len();
    self.nodes.push(Node::new(obj, s_k.clone()));
    self.hash.insert(s_k.clone(), new_idx);

    // Find the left node's index (None = insert at head position)
    let left_idx = left.and_then(|s4v| self.hash.get(&s4v).copied());

    // Starting position: immediately right of the left node
    let mut prev = left_idx;
    let mut cur = match left_idx {
        None => self.head,
        Some(idx) => self.nodes[idx].link,
    };

    // Skip nodes with strictly higher priority (they were concurrently inserted
    // at the same position and win over us). Stop at the first node with lower
    // priority — insert before it. (Definition 9, Algorithm 8)
    while let Some(c) = cur {
        if self.nodes[c].s_k.precedes(&s_k) {
            // c.s_k has LOWER priority than s_k → stop, insert before c
            break;
        }
        prev = Some(c);
        cur = self.nodes[c].link;
    }

    // Link new node between prev and cur
    self.nodes[new_idx].link = cur;
    match prev {
        None => self.head = Some(new_idx),
        Some(p) => self.nodes[p].link = Some(new_idx),
    }
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test test_empty_rga_is_empty_string test_single_remote_insert_at_head test_sequential_remote_inserts test_concurrent_insert_same_position -- --nocapture 2>&1 | tail -20
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 5: Commit**

```bash
git add src/crdt/rga.rs
git commit -m "feat(crdt): implement remote_insert (Algorithm 8), findlist, to_string"
```

---

## Task 3 — Rga::local_insert()

**Files:**
- Modify: `src/crdt/rga.rs`

`local_insert` generates a S4Vector-based `Op::Insert` and applies it locally by calling `remote_insert`. Position is 0-indexed: pos=0 = insert at head, pos=N = insert after the N-th visible char.

- [ ] **Step 1: Replace the `test_single_insert` and `test_concurrent_insert_same_position_local` stubs**

```rust
#[test]
fn test_single_insert() {
    let mut rga = Rga::new(1, 0);
    let op = rga.local_insert(0, 'a').unwrap();
    assert_eq!(rga.to_string(), "a");
    // Op must carry the correct structure
    match op {
        Op::Insert { left, obj, s_k } => {
            assert_eq!(left, None);     // inserted at head
            assert_eq!(obj, 'a');
            assert_eq!(s_k.sid, 1);     // this site's id
        }
        _ => panic!("expected Insert op"),
    }
}

#[test]
fn test_concurrent_insert_same_position_local() {
    let mut rga1 = Rga::new(1, 0);
    let mut rga2 = Rga::new(2, 0);

    let op1 = rga1.local_insert(0, 'a').unwrap(); // s_k.sid = 1
    let op2 = rga2.local_insert(0, 'b').unwrap(); // s_k.sid = 2

    // Cross-apply: rga1 already has 'a', apply rga2's insert
    rga1.apply(op2.clone());
    // Cross-apply: rga2 already has 'b', apply rga1's insert
    rga2.apply(op1.clone());

    // Both must converge to the same string
    assert_eq!(rga1.to_string(), rga2.to_string());
    // sid=2 has higher priority → 'b' wins → appears first
    assert_eq!(rga1.to_string(), "ba");
}
```

- [ ] **Step 2: Run — verify tests fail**

```bash
cargo test test_single_insert test_concurrent_insert_same_position_local 2>&1 | tail -10
```

Expected: panics on `todo!()` in `local_insert`.

- [ ] **Step 3: Implement local_insert**

Replace the `local_insert` `todo!()`:

```rust
/// Insert `obj` after pos visible characters (pos=0 = insert at head).
/// Generates a S4Vector op with this site's id, applies it locally, returns it for broadcast.
pub fn local_insert(&mut self, pos: usize, obj: char) -> Option<Op> {
    let left_s4v = self.findlist(pos).map(|idx| self.nodes[idx].s_k.clone());
    let s_k = self.next_s4vector();
    self.remote_insert(left_s4v.clone(), obj, s_k.clone());
    Some(Op::Insert { left: left_s4v, obj, s_k })
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test test_single_insert test_concurrent_insert_same_position_local -- --nocapture 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 5: Commit**

```bash
git add src/crdt/rga.rs
git commit -m "feat(crdt): implement local_insert"
```

---

## Task 4 — Rga::local_delete() + remote_delete() (Algorithm 9)

**Files:**
- Modify: `src/crdt/rga.rs`

Delete tombstones a node (sets `tombstone=true`, updates `s_p` for precedence against concurrent Updates). Position is 0-indexed: pos=0 = delete first visible char.

- [ ] **Step 1: Replace the `test_delete_then_concurrent_insert` and `test_convergence_two_orderings` stubs**

```rust
/// Example 2 from the paper: delete a node, then a concurrent insert lands
/// after the same (now-tombstoned) node. The insert must still be placed
/// correctly relative to the tombstone.
#[test]
fn test_delete_then_concurrent_insert() {
    let mut rga1 = Rga::new(1, 0);
    let mut rga2 = Rga::new(2, 0);

    // Both start with "ab"
    let op_a = rga1.local_insert(0, 'a').unwrap();
    let op_b = rga1.local_insert(1, 'b').unwrap();
    rga2.apply(op_a);
    rga2.apply(op_b);
    assert_eq!(rga1.to_string(), "ab");
    assert_eq!(rga2.to_string(), "ab");

    // Concurrent: site 1 deletes 'a' (pos 0), site 2 inserts 'x' after 'a' (pos 1)
    let del_op = rga1.local_delete(0).unwrap();
    let ins_op = rga2.local_insert(1, 'x').unwrap(); // insert after 'a'

    rga1.apply(ins_op);
    rga2.apply(del_op);

    assert_eq!(rga1.to_string(), rga2.to_string());
    assert_eq!(rga1.to_string(), "xb"); // 'a' tombstoned; 'x' inserted after tombstone, before 'b'
}

/// Apply two concurrent ops in opposite orders — both RGAs must converge.
#[test]
fn test_convergence_two_orderings() {
    let mut rga1 = Rga::new(1, 0);
    let mut rga2 = Rga::new(2, 0);

    let op1 = rga1.local_insert(0, 'a').unwrap();
    let op2 = rga2.local_insert(0, 'b').unwrap();

    // Order A: rga1 already has 'a', apply 'b'
    rga1.apply(op2.clone());
    // Order B: rga2 already has 'b', apply 'a'
    rga2.apply(op1.clone());

    assert_eq!(rga1.to_string(), rga2.to_string());

    // Now each site deletes its own char and applies the other's delete
    let del1 = rga1.local_delete(1).unwrap(); // rga1's text is "ba", delete pos 1 = 'a'
    let del2 = rga2.local_delete(0).unwrap(); // rga2's text is "ba", delete pos 0 = 'b'
    rga1.apply(del2);
    rga2.apply(del1);

    assert_eq!(rga1.to_string(), rga2.to_string());
    assert_eq!(rga1.to_string(), ""); // both chars deleted
}
```

- [ ] **Step 2: Run — verify tests fail**

```bash
cargo test test_delete_then_concurrent_insert test_convergence_two_orderings 2>&1 | tail -10
```

Expected: panics on `todo!()`.

- [ ] **Step 3: Implement remote_delete and local_delete**

Replace both `todo!()`s:

```rust
/// Algorithm 9 (Roh et al. 2011).
/// Tombstone the node identified by `target`, record `s_o` as new s_p.
pub fn remote_delete(&mut self, target: S4Vector, s_o: S4Vector) {
    if let Some(&idx) = self.hash.get(&target) {
        self.nodes[idx].tombstone = true;
        self.nodes[idx].s_p = s_o;
    }
}

/// Delete the visible character at 0-indexed position pos (pos=0 = first char).
/// Returns the Op to broadcast, or None if pos is out of bounds.
pub fn local_delete(&mut self, pos: usize) -> Option<Op> {
    let target_idx = self.findlist(pos + 1)?; // findlist is 1-indexed; pos+1 gives the right node
    let target = self.nodes[target_idx].s_k.clone();
    let s_k = self.next_s4vector();
    self.remote_delete(target.clone(), s_k.clone());
    Some(Op::Delete { target, s_k })
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test test_delete_then_concurrent_insert test_convergence_two_orderings -- --nocapture 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 5: Commit**

```bash
git add src/crdt/rga.rs
git commit -m "feat(crdt): implement local_delete + remote_delete (Algorithm 9)"
```

---

## Task 5 — Rga::local_update() + remote_update() (Algorithm 10)

**Files:**
- Modify: `src/crdt/rga.rs`

Update replaces a node's character if the incoming op has higher precedence (s_o succeeds node.s_p). Delete always beats Update — if node is already tombstoned, the update is silently dropped.

- [ ] **Step 1: Replace the `test_update_loses_to_delete` stub**

```rust
/// Concurrent Delete and Update on the same node: Delete wins (Algorithm 10).
#[test]
fn test_update_loses_to_delete() {
    let mut rga1 = Rga::new(1, 0);
    let mut rga2 = Rga::new(2, 0);

    // Both start with "a"
    let op_a = rga1.local_insert(0, 'a').unwrap();
    rga2.apply(op_a);

    // Concurrent: site 1 deletes 'a', site 2 updates 'a' → 'z'
    let del_op = rga1.local_delete(0).unwrap();
    let upd_op = rga2.local_update(0, 'z').unwrap();

    // Apply cross-ops
    rga1.apply(upd_op); // site 1 deleted first, update should lose
    rga2.apply(del_op); // site 2 updated first, but delete wins

    assert_eq!(rga1.to_string(), "");
    assert_eq!(rga2.to_string(), "");
}
```

- [ ] **Step 2: Run — verify test fails**

```bash
cargo test test_update_loses_to_delete 2>&1 | tail -10
```

- [ ] **Step 3: Implement remote_update and local_update**

```rust
/// Algorithm 10 (Roh et al. 2011).
/// Replace obj if s_o has HIGHER priority than the node's current s_p.
/// If the node is already tombstoned (Delete won), do nothing.
pub fn remote_update(&mut self, target: S4Vector, obj: char, s_o: S4Vector) {
    if let Some(&idx) = self.hash.get(&target) {
        if self.nodes[idx].is_tombstone() {
            return; // Delete wins over Update (Algorithm 10, line 1)
        }
        if self.nodes[idx].s_p.precedes(&s_o) {
            // s_o has HIGHER priority than current s_p → apply update
            self.nodes[idx].obj = obj;
            self.nodes[idx].s_p = s_o;
        }
        // else: s_o has lower priority → do nothing (another op already won)
    }
}

/// Replace the visible character at 0-indexed position pos with obj.
pub fn local_update(&mut self, pos: usize, obj: char) -> Option<Op> {
    let target_idx = self.findlist(pos + 1)?;
    let target = self.nodes[target_idx].s_k.clone();
    let s_k = self.next_s4vector();
    self.remote_update(target.clone(), obj, s_k.clone());
    Some(Op::Update { target, obj, s_k })
}
```

- [ ] **Step 4: Run all CRDT tests**

```bash
cargo test -- --nocapture 2>&1 | grep -E "test |FAILED|ok\."
```

Expected: all non-`todo!()` tests pass. The stubs still present (`test_single_insert` etc.) will still be `todo!()` — that's fine, they're filled in during testing above.

Actually, at this point all tests should be either passing or `todo!()` panics (not failures). Let's clean up and replace remaining stubs:

Replace the remaining stub tests that were left as `todo!()` with the finalized versions (they're now just duplicates of what we already added):

```rust
#[test]
fn test_single_insert() {
    // Already covered in Task 3's test_single_insert — this one is superseded.
    // Delete this stub entirely or keep as doc reference.
    let mut rga = Rga::new(1, 0);
    rga.local_insert(0, 'a').unwrap();
    assert_eq!(rga.to_string(), "a");
}
```

Delete or replace remaining `todo!()` stubs since they were superseded by the tests added above.

- [ ] **Step 5: Run full test suite**

```bash
cargo test 2>&1 | tail -15
```

Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 6: Commit**

```bash
git add src/crdt/rga.rs
git commit -m "feat(crdt): implement local_update + remote_update (Algorithm 10), all RGA tests pass"
```

---

## Task 6 — Server: Registry redesign

**Files:**
- Rewrite: `src/server/registry.rs`

Replace the channel-only Registry with one that holds per-client RGA instances and handles the full sync algorithm. This is the core server CRDT logic.

- [ ] **Step 1: Replace src/server/registry.rs entirely**

```rust
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use serde::Serialize;
use crate::crdt::{Rga, Op};

pub type Rx = mpsc::UnboundedReceiver<String>;
type Tx  = mpsc::UnboundedSender<String>;

/// A position-based op from the client — no S4Vectors needed in the browser.
#[derive(Debug)]
pub struct ClientOp {
    pub op:  String,       // "insert", "delete", or "update"
    pub pos: usize,        // 0-indexed visible-character position
    pub ch:  Option<char>, // present for insert/update, absent for delete
}

struct ClientInfo {
    tx:           Tx,
    rga:          Rga,
    pending_ops:  Vec<Op>, // remote ops queued until this client's next sync
}

/// Server → client: broadcast after every sync.
#[derive(Serialize)]
struct StateMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    text:    &'a str,
    clients: usize,
}

#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HashMap<u64, ClientInfo>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client. Returns (rx channel, current canonical text for init msg).
    /// New client starts with an empty RGA; the caller sends current text as `init`.
    pub async fn connect(&self, site_id: u64) -> (Rx, String) {
        let mut clients = self.inner.write().await;
        let (tx, rx) = mpsc::unbounded_channel::<String>();

        let current_text = clients
            .values()
            .next()
            .map(|c| c.rga.to_string())
            .unwrap_or_default();

        clients.insert(site_id, ClientInfo {
            tx,
            rga: Rga::new(site_id, 0),
            pending_ops: Vec::new(),
        });

        println!("[INFO] site {} connected ({} clients)", site_id, clients.len());
        (rx, current_text)
    }

    /// Remove a disconnected client.
    pub async fn disconnect(&self, site_id: u64) {
        let mut clients = self.inner.write().await;
        clients.remove(&site_id);
        println!("[INFO] site {} disconnected ({} clients)", site_id, clients.len());
    }

    /// Core CRDT sync algorithm (spec §4).
    ///
    /// Step 1: apply incoming local ops to X's RGA (uses X's s_k / sid).
    /// Step 2: queue the resulting S4Vector ops into every OTHER client's pending_ops.
    /// Step 3: drain X's accumulated pending remote ops and apply them to X's RGA.
    /// Step 4: broadcast canonical state to all clients.
    pub async fn process_sync(&self, from_site: u64, ops: Vec<ClientOp>) {
        let mut clients = self.inner.write().await;

        if !clients.contains_key(&from_site) {
            eprintln!("[WARN] process_sync for unknown site {}", from_site);
            return;
        }

        // Step 1 — local ops → S4Vector ops
        let mut s4v_ops: Vec<Op> = Vec::new();
        for client_op in ops {
            let client = clients.get_mut(&from_site).unwrap();
            let result = match client_op.op.as_str() {
                "insert" => match client_op.ch {
                    Some(ch) => client.rga.local_insert(client_op.pos, ch),
                    None => {
                        eprintln!("[WARN] insert from site {} missing char", from_site);
                        None
                    }
                },
                "delete" => client.rga.local_delete(client_op.pos),
                "update" => match client_op.ch {
                    Some(ch) => client.rga.local_update(client_op.pos, ch),
                    None => None,
                },
                other => {
                    eprintln!("[WARN] unknown op '{}' from site {}", other, from_site);
                    None
                }
            };
            if let Some(op) = result {
                s4v_ops.push(op);
            }
        }

        // Step 2 — queue to all other clients
        for (site_id, client) in clients.iter_mut() {
            if *site_id != from_site {
                client.pending_ops.extend(s4v_ops.iter().cloned());
            }
        }

        // Step 3 — drain X's pending remote ops
        let pending = std::mem::take(&mut clients.get_mut(&from_site).unwrap().pending_ops);
        let client = clients.get_mut(&from_site).unwrap();
        for op in pending {
            client.rga.apply(op);
        }

        // Step 4 — broadcast canonical state to all
        let canonical = clients[&from_site].rga.to_string();
        let count = clients.len();
        let json = serde_json::to_string(&StateMsg {
            msg_type: "state",
            text: &canonical,
            clients: count,
        })
        .expect("serialize StateMsg");

        for client in clients.values() {
            if client.tx.send(json.clone()).is_err() {
                // Recipient will be cleaned up when their WS task notices the disconnect
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build 2>&1 | head -40
```

Expected: errors about `AppState` referencing old `Registry` API, but `registry.rs` itself should compile cleanly. Fix any import issues in registry.rs first.

- [ ] **Step 3: Commit**

```bash
git add src/server/registry.rs
git commit -m "feat(server): redesign Registry with per-client RGA + process_sync algorithm"
```

---

## Task 7 — WebSocket handler: send/recv tasks + protocol types

**Files:**
- Modify: `src/server/handler.rs`

Implement the two async tasks inside `handle_socket`. Add protocol type definitions. Update `AppState::new()` to remove `session` field (not needed; hardcoded 0 in Registry).

- [ ] **Step 1: Replace src/server/handler.rs entirely**

```rust
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::server::registry::{ClientOp, Registry, Rx};

/// Client → server: a batch of position-based ops sent on each sync interval.
#[derive(Debug, Deserialize)]
struct ClientOpsMsg {
    #[serde(rename = "type")]
    msg_type: String,
    ops: Vec<ClientOpJson>,
}

#[derive(Debug, Deserialize)]
struct ClientOpJson {
    op:  String,
    pos: usize,
    #[serde(rename = "char")]
    ch:  Option<char>,
}

/// Server → client: sent once on connect.
#[derive(Serialize)]
struct InitMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    site_id:  u64,
    text:     &'a str,
}

#[derive(Clone)]
pub struct AppState {
    pub registry:     Registry,
    pub next_site_id: Arc<AtomicU64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            registry:     Registry::new(),
            next_site_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn assign_site_id(&self) -> u64 {
        self.next_site_id.fetch_add(1, Ordering::Relaxed)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn serve_index() -> &'static str {
    include_str!("../../static/index.html")
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let site_id = state.assign_site_id();
    let (rx, current_text) = state.registry.connect(site_id).await;
    let (mut sink, mut stream) = socket.split();

    // Send init message immediately
    let init_json = serde_json::to_string(&InitMsg {
        msg_type: "init",
        site_id,
        text: &current_text,
    })
    .expect("serialize InitMsg");

    if sink.send(WsMessage::Text(init_json.into())).await.is_err() {
        state.registry.disconnect(site_id).await;
        return;
    }

    // Outbound task: forward JSON strings from the registry channel to this WebSocket.
    let mut send_task = tokio::spawn(async move {
        let mut rx: Rx = rx;
        while let Some(json) = rx.recv().await {
            if sink.send(WsMessage::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound task: receive op batches from this client, run the CRDT sync algorithm.
    let registry = state.registry.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            let text = match msg {
                Ok(WsMessage::Text(t)) => t,
                Ok(WsMessage::Close(_)) | Err(_) => break,
                _ => continue,
            };

            let parsed: ClientOpsMsg = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[WARN] bad message from site {}: {}", site_id, e);
                    continue;
                }
            };

            if parsed.msg_type != "ops" {
                eprintln!("[WARN] unknown msg type '{}' from site {}", parsed.msg_type, site_id);
                continue;
            }

            let ops: Vec<ClientOp> = parsed
                .ops
                .into_iter()
                .map(|o| ClientOp { op: o.op, pos: o.pos, ch: o.ch })
                .collect();

            registry.process_sync(site_id, ops).await;
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    state.registry.disconnect(site_id).await;
}
```

- [ ] **Step 2: Build — verify zero errors**

```bash
cargo build 2>&1
```

Expected: clean build (possible dead-code warnings from unused `next` field in Node, that's fine).

- [ ] **Step 3: Run server and do a smoke test with wscat or curl**

```bash
cargo run &
sleep 1
curl -s http://localhost:3000/   # should return HTML
# If wscat is available:
# wscat -c ws://localhost:3000/ws
# Send: {"type":"ops","ops":[{"op":"insert","pos":0,"char":"h"}]}
# Expect: {"type":"state","text":"h","clients":1}
kill %1
```

- [ ] **Step 4: Commit**

```bash
git add src/server/handler.rs
git commit -m "feat(server): implement WebSocket send/recv tasks, init message, protocol types"
```

---

## Task 8 — Frontend: static/index.html

**Files:**
- Rewrite: `static/index.html`

Dead-simple textarea + sync interval slider + status bar. All JavaScript inline. No framework, no build step. Implements the client-side protocol from the spec.

- [ ] **Step 1: Replace static/index.html**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>RGA Collaborative Editor</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: monospace; background: #1e1e1e; color: #d4d4d4; display: flex; flex-direction: column; align-items: center; padding: 2rem; min-height: 100vh; }
    h2 { margin-bottom: 1rem; font-size: 1.1rem; letter-spacing: 0.05em; color: #9cdcfe; }
    #editor { width: 700px; height: 300px; background: #252526; color: #d4d4d4; border: 1px solid #3c3c3c; padding: 0.75rem; font-family: monospace; font-size: 0.95rem; resize: vertical; outline: none; }
    #editor:focus { border-color: #007acc; }
    #status { width: 700px; margin-top: 0.5rem; font-size: 0.8rem; color: #858585; display: flex; justify-content: space-between; }
    #controls { width: 700px; margin-top: 1rem; display: flex; align-items: center; gap: 1rem; font-size: 0.85rem; }
    #interval-label { white-space: nowrap; }
    #interval { flex: 1; accent-color: #007acc; }
  </style>
</head>
<body>
  <h2>RGA Collaborative Editor</h2>
  <textarea id="editor" placeholder="Start typing..."></textarea>
  <div id="status">
    <span id="conn-status">🔴 Connecting...</span>
    <span id="sync-status">-</span>
  </div>
  <div id="controls">
    <label id="interval-label">Sync interval: <strong id="interval-val">1000</strong>ms</label>
    <input type="range" id="interval" min="100" max="5000" step="100" value="1000" />
  </div>

  <script>
    const editor      = document.getElementById('editor');
    const connStatus  = document.getElementById('conn-status');
    const syncStatus  = document.getElementById('sync-status');
    const intervalEl  = document.getElementById('interval');
    const intervalVal = document.getElementById('interval-val');

    let ws;
    let siteId       = null;
    let prevText     = '';
    let pendingOps   = [];
    let intervalMs   = 1000;
    let syncTimer    = null;
    let countdown    = 0;
    let cdTimer      = null;

    // -----------------------------------------------------------------------
    // Text diff — generates minimal insert/delete op list (LCS-based prefix/suffix)
    // Deletes are emitted in reverse order (high → low) so positions stay stable.
    // Inserts are in forward order matching positions in the new string.
    // -----------------------------------------------------------------------
    function computeOps(oldText, newText) {
      if (oldText === newText) return [];
      const ops = [];

      // Common prefix
      let pre = 0;
      while (pre < oldText.length && pre < newText.length && oldText[pre] === newText[pre]) pre++;

      // Common suffix
      let oldEnd = oldText.length, newEnd = newText.length;
      while (oldEnd > pre && newEnd > pre && oldText[oldEnd - 1] === newText[newEnd - 1]) {
        oldEnd--; newEnd--;
      }

      // Deletes (reverse order)
      for (let i = oldEnd - 1; i >= pre; i--) ops.push({ op: 'delete', pos: i });

      // Inserts (forward order; positions reference the new string)
      for (let i = pre; i < newEnd; i++) ops.push({ op: 'insert', pos: i, char: newText[i] });

      return ops;
    }

    // -----------------------------------------------------------------------
    // Sync — send pending ops to server
    // -----------------------------------------------------------------------
    function doSync() {
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      const currentText = editor.value;
      const newOps = computeOps(prevText, currentText);
      pendingOps.push(...newOps);
      prevText = currentText;

      if (pendingOps.length > 0) {
        ws.send(JSON.stringify({ type: 'ops', ops: pendingOps }));
        pendingOps = [];
      }
      countdown = intervalMs;
    }

    function startSyncTimer() {
      if (syncTimer) clearInterval(syncTimer);
      syncTimer = setInterval(doSync, intervalMs);
      countdown = intervalMs;
    }

    // Countdown display — updates every 100ms
    setInterval(() => {
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      countdown = Math.max(0, countdown - 100);
      syncStatus.textContent = `Next sync: ${(countdown / 1000).toFixed(1)}s | Site #${siteId ?? '?'}`;
    }, 100);

    // -----------------------------------------------------------------------
    // WebSocket
    // -----------------------------------------------------------------------
    function connect() {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws';
      ws = new WebSocket(`${proto}://${location.host}/ws`);

      ws.onopen = () => {
        connStatus.textContent = '🟢 Connected';
        startSyncTimer();
      };

      ws.onclose = () => {
        connStatus.textContent = '🔴 Disconnected';
        if (syncTimer) clearInterval(syncTimer);
        // Reconnect after 2s
        setTimeout(connect, 2000);
      };

      ws.onerror = () => {
        connStatus.textContent = '🔴 Error';
      };

      ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);

        if (msg.type === 'init') {
          siteId = msg.site_id;
          applyState(msg.text);
          connStatus.textContent = `🟢 Connected · Site #${siteId}`;
          return;
        }

        if (msg.type === 'state') {
          applyState(msg.text);
          connStatus.textContent = `🟢 Connected · Site #${siteId} · ${msg.clients} client${msg.clients !== 1 ? 's' : ''}`;
        }
      };
    }

    function applyState(newText) {
      if (editor.value === newText) return;
      const sel = editor.selectionStart;
      editor.value = newText;
      const newSel = Math.min(sel, newText.length);
      editor.setSelectionRange(newSel, newSel);
      prevText = newText;
      // Clear pending ops — server is now canonical
      pendingOps = [];
    }

    // -----------------------------------------------------------------------
    // Slider
    // -----------------------------------------------------------------------
    intervalEl.addEventListener('input', () => {
      intervalMs = parseInt(intervalEl.value, 10);
      intervalVal.textContent = intervalMs;
      startSyncTimer();
    });

    connect();
  </script>
</body>
</html>
```

- [ ] **Step 2: Start the server and test in two browser tabs**

```bash
cargo run
```

Open `http://localhost:3000` in two browser tabs.

Manual test checklist:
- [ ] Both tabs show the same text after typing in either
- [ ] Setting one tab to 3000ms interval and typing in both shows delayed sync
- [ ] After delay expires, both tabs show the same merged text
- [ ] Status bar shows correct site ID and client count
- [ ] Disconnecting one tab (close) decrements the client count on the other

- [ ] **Step 3: Commit**

```bash
git add static/index.html
git commit -m "feat(frontend): textarea editor with sync interval slider and status bar"
```

---

## Task 9 — README.md

**Files:**
- Create: `README.md`

All sections required by the assignment (Norwegian spec). Academic content woven into the technical descriptions.

- [ ] **Step 1: Create README.md at repo root**

```markdown
# RGA Collaborative Editor

[![CI](https://github.com/<org>/<repo>/actions/workflows/ci.yml/badge.svg)](https://github.com/<org>/<repo>/actions)

## Introduction

A browser-based collaborative text editor backed by a **Replicated Growable Array (RGA)** CRDT implemented from scratch in Rust.

Traditional collaborative editors use Operational Transformation (OT), which requires a central server to serialize and transform concurrent operations. RGA instead uses *tombstone-based conflict-free replication*: each character carries a globally unique **S4Vector** identifier (session, site, sum, sequence — Definition 9 in Roh et al. 2011). Concurrent inserts at the same position are resolved deterministically by comparing S4Vectors — no locking, no transformation functions, no central arbiter.

**Properties guaranteed by the RGA algorithm:**
- **Operation Commutativity (OC):** applying any two ops in either order produces the same result.
- **Precedence Transitivity (PT):** the insertion ordering is globally consistent across all sites.

These two properties together guarantee **eventual consistency** — after all pending ops are delivered, every site converges to the same document.

## Implemented Functionality

- **RGA Insert** (Algorithm 8) — concurrent inserts at the same position resolved by S4Vector ordering.
- **RGA Delete** (Algorithm 9) — tombstone-based deletion; deleted nodes remain in the linked list.
- **RGA Update** (Algorithm 10) — character replacement with Delete-wins precedence.
- **WebSocket server** — `axum` + `tokio` server assigning each client a unique `site_id`; per-client RGA instances ensure each site's S4Vectors carry the correct `sid`.
- **Sync interval slider** — models a partitioned / slow network. Higher interval = longer disconnect between syncs; convergence is demonstrated when both clients finally sync.
- **Multi-client** — N concurrent browser tabs.

## Future Work / Known Weaknesses

- **Tombstone accumulation** — deleted nodes are never purged. Document memory grows monotonically. Section 5.6 of the paper describes a purging protocol but it is not implemented.
- **Late-joiner state** — a client connecting after typing has begun receives the current text as display only; their RGA starts empty, so they cannot delete pre-existing content via CRDT ops.
- **No persistent history** — restarting the server clears the document.
- **Single-server only** — the server is the communication hub. True peer-to-peer distribution is not supported.
- **No undo/redo** — the spec explicitly excludes this.

## External Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `tokio` | 1 | Async runtime; task spawning, channels, TCP listener |
| `axum` | 0.7 | HTTP router, WebSocket upgrade handler (`ws` feature) |
| `serde` | 1 | Derive macros for serialization (`#[derive(Serialize, Deserialize)]`) |
| `serde_json` | 1 | JSON encoding/decoding of WebSocket messages |
| `futures-util` | 0.3 | `SinkExt` / `StreamExt` for async WebSocket read/write |

No external CRDT libraries are used. All RGA logic is hand-implemented from Roh et al. 2011.

## Installation

Requires Rust (stable, 2024 edition). Install via [rustup](https://rustup.rs/).

```bash
git clone <repo-url>
cd idatt2104-mappe-2026
cargo build --release
```

## Usage

```bash
cargo run --release
```

Open `http://localhost:3000` in two or more browser tabs.

To demonstrate convergence with a network partition:
1. Set one tab's sync interval slider to 3000 ms.
2. Type in both tabs simultaneously.
3. Observe that the high-interval tab lags behind.
4. After 3 seconds both tabs display the same merged text.

## Running Tests

```bash
cargo test
```

Unit tests are in `src/crdt/rga.rs` (module `tests`). They verify:
- Single and sequential inserts
- Concurrent inserts at the same position (dOPT puzzle, Example 1 from the paper)
- Delete + concurrent insert (Example 2 from the paper)
- Convergence when ops are applied in opposite orders (OC property)
- Delete-wins over Update (Algorithm 10)

## API Documentation

Run `cargo doc --open` for generated API docs.

## Attribution

All RGA algorithms are taken directly from:

> Roh, H. G., Jeon, M., Kim, J. S., & Lee, J. (2011).
> *Replicated abstract data types: Building blocks for collaborative applications.*
> Journal of Parallel and Distributed Computing, 71(3), 354–368.

Specific mappings:
- `S4Vector::precedes()` — Definition 9
- `Rga::remote_insert()` — Algorithm 8
- `Rga::remote_delete()` — Algorithm 9
- `Rga::remote_update()` — Algorithm 10
- `Rga::findlist()` — Algorithm 4
- SVI hash table scheme — Section 5.4
```

- [ ] **Step 2: Replace `<org>/<repo>` with actual GitHub path**

```bash
git remote get-url origin
# e.g. https://github.com/AlexElton/idatt2104-mappe-2026
# Update the CI badge URL in README.md to match
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add full README with all required assignment sections"
```

---

## Self-Review Checklist

After all tasks are done, run:

```bash
cargo test
cargo build --release
```

And verify manually:
- [ ] Two tabs open → type in both → sync interval fires → same text on both sides
- [ ] One tab at 3000ms interval → type in both → demonstrate delay → converge
- [ ] `cargo test` outputs `test result: ok. N passed; 0 failed`
- [ ] `cargo build --release` has zero errors
- [ ] `git log --oneline` shows one commit per task

**Spec coverage check:**

| Spec section | Covered by task |
|---|---|
| S4Vector::precedes (Def 9) | Task 1 |
| Node struct | Task 1 |
| findlist (Algorithm 4) | Task 2 |
| remote_insert (Algorithm 8) | Task 2 |
| to_string | Task 2 |
| local_insert | Task 3 |
| Concurrent insert convergence test | Task 2+3 |
| remote_delete (Algorithm 9) | Task 4 |
| local_delete | Task 4 |
| Delete+insert convergence test | Task 4 |
| remote_update (Algorithm 10) | Task 5 |
| local_update | Task 5 |
| Delete-wins test | Task 5 |
| Registry + per-client RGA | Task 6 |
| process_sync algorithm | Task 6 |
| broadcast_state | Task 6 |
| WS send task | Task 7 |
| WS recv task | Task 7 |
| Init message | Task 7 |
| Protocol types (ClientOpsMsg, InitMsg) | Task 7 |
| textarea + slider + status | Task 8 |
| computeOps diff | Task 8 |
| applyState with cursor preservation | Task 8 |
| All README sections | Task 9 |
| Academic attribution | Task 9 |

No gaps found.
