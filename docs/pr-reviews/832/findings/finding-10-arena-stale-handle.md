# Finding #10 — Arena Stale-Handle ABA Problem (No Generation Counters)

**Status:** ⚠️ OPEN — usage contract needs clarification or documentation  
**Severity:** Logic bug — silent wrong data if a stale `GNodeId` is used after node eviction  
**Files:** `packages/mudlark/src/arena.rs` · `packages/mudlark/src/graph.rs`

---

## Description

`Arena<T>` is a Vec-backed free-list allocator with **no generation tracking**. When a slot is
deallocated, its index is pushed onto the free list and immediately available for reuse. A subsequent
`alloc()` call can hand that same slot index to a completely different node.

`Arena::get()` and `Arena::get_mut()` assert occupancy via `is_occupied()`:

```rust
pub fn get(&self, index: usize) -> &T {
    debug_assert!(self.is_occupied(index), "Arena::get: slot {index} is not occupied");
    &self.slots[index]
}
```

This guard only checks whether **some** node occupies the slot. It does **not** check whether it is
the **same** node the handle was issued for. If node A was evicted and slot i was recycled for node B,
`is_occupied(i)` returns `true` — but the data returned now describes B, not A.

This is the classic **ABA problem**: the slot went A → free → B, and the old handle for A
silently resolves to B with no indication of the substitution.

---

## Affected Public Methods

The following `GvGraph` methods accept a `GNodeId` and use `is_occupied()` as their only guard:

| Method             | Guard           | Behaviour on stale handle                     |
| ------------------ | --------------- | --------------------------------------------- |
| `gnode_info()`     | `is_occupied()` | Returns `Some(Node)` for the **new occupant** |
| `gnode_children()` | `is_occupied()` | Returns children of the **new occupant**      |
| `is_ancestor_of()` | walks tree      | May follow wrong edges silently               |

---

## Concrete Scenario

1. User calls `plateaus()` — iterates over `BasisElement` values, each carrying a `GNodeId`.
   User caches these in a `HashMap<GNodeId, MyTrackerState>`.
2. User calls `observe(...)` — triggers eviction. Some of the cached G-nodes are freed.
3. A subsequent split — freed arena slots are recycled for new G-nodes.
4. User calls `gnode_info(cached_id)`:
   - `is_occupied()` returns `true` (the slot now holds a different node).
   - Returns a `Node` describing the **wrong G-node**. No error is signalled.

This is a logic bug, not a memory-safety bug (Rust prevents the actual UB from materialising),
but it silently corrupts the caller's view of the tree.

---

## Question for Cameron

**Is caching `GNodeId` values across structural mutations a supported use case?**

- If **no (ephemeral only):** The usage contract should be explicitly documented. Something like:

  > _`GNodeId` values are valid only until the next call to `observe()`, `decay()`, or
  > `check_evictions()`. Storing a `GNodeId` obtained from `plateaus()`, `sample()`, or prior
  > `gnode_info()` calls and reusing it after a structural mutation has undefined logical semantics._

  In this case the `is_occupied()` guard in `gnode_info()` is misleadingly protective — it appears
  to return `None` for dead handles but actually cannot distinguish "dead" from "recycled for a
  different node".

- If **yes (stable across mutations):** A **generation counter** is needed. Standard solutions:
  - [`slotmap`](https://docs.rs/slotmap/) — uses keyed access, safe after deletion.
  - [`generational-arena`](https://docs.rs/generational-arena/) — adds generation IDs to handles.
  - Custom: add `generation: u32` to `GNodeId` and a matching `u32` per arena slot.

---

## Fix Options

### Option A — Document the ephemeral contract (no code change)

Add a `# Validity` section to `GNodeId`'s doc comment:

```rust
/// # Validity
///
/// A `GNodeId` obtained from any read operation ([`plateaus`], [`sample`], [`gnode_info`], etc.)
/// is valid only until the next structural mutation of the same `GvGraph` (`observe`, `decay`,
/// `check_evictions`). Using a stale handle after a mutation has undefined logical semantics —
/// `gnode_info` may return `Some` for a different node that reused the same arena slot.
```

Update `gnode_info()`, `gnode_children()` and `is_ancestor_of()` doc comments to reflect this.

### Option B — Add generation counters to handles and arena

Add a `generation: u32` word to `GNodeId` (still 8 bytes; `Option<GNodeId>` loses niche
optimisation unless packed into the upper bits of the index word):

```rust
pub struct GNodeId { index: u32, generation: u32 }
```

Store a matching `generation: u32` per arena slot (either in `GNode` or in a parallel
`Vec<u32>` on `Arena`). Increment on each `alloc()`. The guard becomes:

```rust
fn is_same_generation(&self, index: usize, gen: u32) -> bool {
    self.generation[index] == gen
}
```

`gnode_info()` then returns `None` for any stale handle. This is a **breaking change** to the
`serde` serialization format for `GNodeId`.

### Option C — Maintain an epoch on the graph

Increment a `u64` epoch counter on every structural mutation. Embed the creation epoch in
`GNodeId`. Callers who care can check `id.epoch == graph.epoch()`. This is cheaper than
per-slot generations if most callers just want "is this handle from the current epoch".

---

## Recommendation

Clarify with Cameron which usage pattern is intended. If `GNodeId` is only ever used within
a single logical "step" (obtain → use → discard, never stored across mutations), Option A
(documentation) is sufficient and zero-cost. If long-lived cached handles are a first-class
use case, generation counters (Option B) are the right solution.

---

## Appendix — Implementation Comparison

The examples below contrast the current custom arena with a generational-arena alternative,
to make the trade-offs concrete.

### Current: Custom Arena (Vec + Free List + Bitset)

```rust
use std::fmt;

#[derive(Debug, Default)]
struct Node {
    value: u32,
    children: Vec<usize>, // bare usize indices — can become dangling
}

struct TreeArena {
    arena: Arena<Node>,
    root: usize,
}

impl TreeArena {
    fn new() -> Self {
        let mut arena = Arena::new();
        let root = arena.alloc(Node::default());
        Self { arena, root }
    }

    fn add_child(&mut self, parent_idx: usize, value: u32) -> usize {
        let child_idx = self.arena.alloc(Node { value, children: vec![] });
        self.arena.get_mut(parent_idx).children.push(child_idx);
        child_idx
    }

    fn remove_node(&mut self, idx: usize) {
        // Slot freed — any stored copy of `idx` elsewhere is now a stale handle.
        // If the slot is later reused, the old idx silently resolves to the new node.
        self.arena.dealloc(idx);
    }
}
```

### Alternative: Generational Arena (Safe Handles)

```rust
use generational_arena::{Arena as GenArena, Index as GenIndex};

#[derive(Debug)]
struct NodeGen {
    value: u32,
    children: Vec<GenIndex>, // generational handles — invalidated on removal
}

struct TreeGen {
    arena: GenArena<NodeGen>,
    root: GenIndex,
}

impl TreeGen {
    fn new() -> Self {
        let mut arena = GenArena::new();
        let root = arena.insert(NodeGen { value: 0, children: vec![] });
        Self { arena, root }
    }

    fn add_child(&mut self, parent: GenIndex, value: u32) -> GenIndex {
        let child = arena.insert(NodeGen { value, children: vec![] });
        self.arena[parent].children.push(child);
        child
    }

    fn remove_node(&mut self, idx: GenIndex) {
        // Old handles to this slot are automatically invalidated.
        // arena.get(old_idx) returns None after removal, even if the slot is reused.
        self.arena.remove(idx);
    }
}
```

### Comparison

| Feature                   | Custom `Arena<T>`                                   | `generational_arena`                      |
| ------------------------- | --------------------------------------------------- | ----------------------------------------- |
| Allocation                | O(1) via free list                                  | O(1) via `insert()`                       |
| Deallocation              | `dealloc()` — old indices may be stale              | `remove()` — old handles auto-invalidated |
| Handle type               | `usize` / `GNodeId(NonZeroU32)` — no generation     | `Index` — embeds generation counter       |
| Access after removal      | `is_occupied()` returns `true` if slot reused (ABA) | `arena.get(old)` → `None` always          |
| Mutable access            | `get_mut()` — debug_assert only                     | `arena[idx]` — guaranteed valid           |
| Safety for cached handles | ⚠️ Stale handle → silently wrong data               | ✅ Stale handle → `None` or panic         |
| Memory growth             | Slots never shrink                                  | Similar                                   |
| Iteration                 | By slot index (ascending)                           | Only live nodes                           |
| Extra cost                | None                                                | +4 bytes per handle, +4 bytes per slot    |

### Further Reading

- [Arenas in Rust](https://donsz.nl/blog/arenas/) — overview of arena patterns and trade-offs
- [`slotmap`](https://docs.rs/slotmap/) — keyed access, safe after deletion
- [`generational-arena`](https://docs.rs/generational-arena/) — generation IDs on handles
