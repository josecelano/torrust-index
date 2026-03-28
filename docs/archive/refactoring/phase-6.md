# Phase 6 — Remove `AtomicU32 cached_depth` from `VNode<V>`

**Opportunity:** [#7](../design-improvement-opportunities.md#7-atomicu32-cached_depth-in-vnode-is-interior-mutability-inside-a-tree)

**Prerequisite:** Phase 5 complete (`VTree` exists and owns the arenas/violations).

**Goal:** Stop each `VNode` from owning its depth via interior mutability.
Move depth knowledge to `VTree` where writes are already synchronised by `&mut self`.

---

## [x] Step 6.1 — Benchmark depth computation before touching anything

**Files touched:** `benches/depth.rs` (created), `Cargo.toml`.

Criterion benchmark added in `benches/depth.rs`. Two scenarios exercise
`v_depth` at different intensities:

| bench                  | config                                         | `cached_depth` baseline |
| ---------------------- | ---------------------------------------------- | ----------------------- |
| `observe/steady_state` | N=8, depth_create=3, depth_evict=5, budget=128 | **~285 ns/iter**        |
| `observe/split_heavy`  | N=8, depth_create=2, depth_evict=4, budget=32  | **~97 ns/iter**         |

(Run on development machine; numbers are relative, not absolute.)

After Step 6.2a the same bench will be re-run. Numbers to fill in after
6.2a:

| bench                  | `cached_depth` | without cache | ratio |
| ---------------------- | -------------- | ------------- | ----- |
| `observe/steady_state` | ~285 ns        | ~291 ns       | 1.02× |
| `observe/split_heavy`  | ~97 ns         | ~97.8 ns      | 1.01× |

**Decision gate:** Trees are architecturally bounded at `depth_evict` (≤ 5
in typical configs). The uncached path walks ≤ 5 parent pointers; with
arena-allocated nodes this is fast. Proceeding with 6.2a (simple removal).

**Validate:** `cargo test --all-features` (no changes yet — just confirm baseline)

---

## [x] Step 6.2a — Remove `cached_depth` (simple path, if benchmark is acceptable)

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

## [x] Step 6.3 — Make `VNode<V>` `Copy` (if `V: Copy`)

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

**Was the `cached_depth` cache providing a measurable speedup?**

No. The benchmark comparison in Step 6.1's table shows ratios of 1.01–1.02×,
which is within measurement noise. The cache was added pre-emptively. The
parent-chain walk is equally fast because trees are architecturally bounded at
`depth_evict` (≤ 5 levels), so the walk visits at most 5 nodes with
cache-warm arena pointers.

**Remaining interior mutability in data types after Phase 6:**

None. A `grep` for `AtomicU{32,64,size}`, `Cell<`, `RefCell<`, `Mutex<`,
`RwLock<` across `src/` returns only hits for `spatial::view::Cell<C, V>` — a
domain struct unrelated to standard-library interior mutability.

**Is `VNode<V>` now `Send + Sync` without needing `Arc`?**

Yes. `VNode<V>` now only contains `V`, `Option<VNodeId>`, and
`VKind<V>`/`Children<V>` — all of which are plain-data enums/arrays with no
heap allocation. `VNode<V>` is `Send + Sync` whenever `V: Send + Sync`, and
`Copy` whenever `V: Copy`.

**Commits:**

| Step | Commit    | Description                                                             |
| ---- | --------- | ----------------------------------------------------------------------- |
| 6.1  | `a94a7ea` | Add criterion depth benchmark (baseline: ~285 ns / ~97 ns)              |
| 6.2a | `213af08` | Remove `cached_depth AtomicU32`, rewrite `v_depth` as parent-chain walk |
| 6.3  | `a59cc89` | Derive `Copy` for `VNode`, `VKind`, `Children` when `V: Copy`           |
