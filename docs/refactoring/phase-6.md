# Phase 6 — Remove `AtomicU32 cached_depth` from `VNode<V>`

**Opportunity:** [#7](../design-improvement-opportunities.md#7-atomicu32-cached_depth-in-vnode-is-interior-mutability-inside-a-tree)

**Prerequisite:** Phase 5 complete (`VTree` exists and owns the arenas/violations).

**Goal:** Stop each `VNode` from owning its depth via interior mutability.
Move depth knowledge to `VTree` where writes are already synchronised by `&mut self`.

---

## [ ] Step 6.1 — Benchmark depth computation before touching anything

**Files touched:** `benches/` (create if it doesn't exist).

If `v_depth(vnodes, id)` already traverses to the root on every call the cache
was added for a reason — measure it before removing it.

```bash
cargo bench --features rand  # or criterion bench if set up
```

If there is no criterion suite yet, write a micro-benchmark:

```rust
// benches/depth.rs  (quick criterion example)
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_depth(c: &mut Criterion) {
    // build a realistic GvGraph (~1000 observations) and call depth() 10k times
    …
}
criterion_group!(benches, bench_depth);
criterion_main!(benches);
```

Record the `ns/iter` baseline in this file:

| depth_tree_size | `cached_depth` time | without-cache time |
| --------------- | ------------------- | ------------------ |
| 64 nodes        | _TBD_               | _TBD_              |
| 512 nodes       | _TBD_               | _TBD_              |
| 4096 nodes      | _TBD_               | _TBD_              |

**Decision gate:** If the uncached path costs ≤ 2× the cached path, proceed.
If it is substantially worse, consider an external `depth_cache: Vec<u32>` +
`depth_dirty: Vec<bool>` in `VTree` instead (see Step 6.2b).

**Validate:** `cargo test --all-features` (no changes yet — just confirm baseline)

---

## [ ] Step 6.2a — Remove `cached_depth` (simple path, if benchmark is acceptable)

**Files touched:** `src/nodes/vnode.rs`, `src/tree/vtree.rs`.

1. Delete the `cached_depth: AtomicU32` field from `VNode<V>`.
2. Delete all reads/writes to `cached_depth`.
3. Rewrite `v_depth` / `VTree::depth` to walk the parent chain (it will already
   be fast for shallow trees, which is the common case).
4. Delete the `Arc` / `AtomicU32` imports from `vnode.rs`.

After this step `VNode<V>` no longer requires `Arc` (or any wrapper) for shared
ownership — it becomes fully `Copy` if `V: Copy`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(nodes): remove cached_depth AtomicU32, walk parent for depth`

---

## [ ] Step 6.2b — External depth cache (fall-back path, if traversal is too slow)

_Only do this step if Step 6.2a profiling showed unacceptable regressions._

Add two `Vec`s to `VTree`:

```rust
pub(crate) struct VTree<V: Accumulator> {
    pub(crate) nodes:         Arena<VNode<V>>,
    pub(crate) root:          Option<VNodeId>,
    pub(crate) violations:    Vec<VNodeId>,
    depth_cache:              Vec<u32>,   // indexed by VNodeId order; rebuilt lazily
    depth_dirty:              bool,
}
```

- `depth()` checks `depth_dirty`; if clean, returns `depth_cache[id]`; if dirty,
  walks and updates.
- Operations that reparent nodes (`invalidate_depth`) set `depth_dirty = true`
  instead of reaching into each `VNode`.

This keeps caching at the `VTree` level (a single owner) rather than inside each
node (interior mutability).

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(nodes): move depth cache from VNode to VTree`

---

## [ ] Step 6.3 — Make `VNode<V>` `Copy` (if `V: Copy`)

**Files touched:** `src/nodes/vnode.rs`.

Once `cached_depth` (the `Arc<AtomicU32>`) is gone the only non-`Copy` field
would be a heap allocation. If `V: Copy` (and any remaining `Vec` fields are
small and stack-resident) you can derive:

```rust
#[derive(Copy, Clone, …)]
pub(crate) struct VNode<V: Accumulator + Copy> {
    …
}
```

Or add a blanket bound only where useful. Check if the type parameter on `VTree`
already requires `Copy` via usage; add the bound there if so.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(nodes): derive Copy for VNode when V: Copy`

---

## Review checkpoint

> _Fill in after completing all steps._
>
> - Was the `cached_depth` cache actually providing a measurable speedup, or had
>   it been added pre-emptively? Record the benchmark numbers in Step 6.1's table.
> - After this phase, is there any remaining interior mutability (`Cell`, `RefCell`,
>   `Mutex`, `AtomicXxx`) in the data types (`GNode`, `VNode`, `Arena`)? List
>   what remains and whether it is justified.
> - Is `VNode<V>` now `Send + Sync` without needing `Arc`?
