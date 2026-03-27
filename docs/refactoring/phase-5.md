# Phase 5 — Introduce `VTree<V>` and move the violations queue

**Opportunities:** [#2](../design-improvement-opportunities.md#2-the-two-trees-have-no-explicit-types),
[#3](../design-improvement-opportunities.md#3-violations-queue-is-scheduling-state-mixed-into-structural-data)

**Prerequisite:** Phase 4 complete (`GTree` exists).

**Goal:** Mirror Phase 4 for the V-tree; move the violations queue inside `VTree`
so that scheduling state has a natural home.

---

## [x] Step 5.1 — Define `VTree<V>` as a struct wrapper

**Files touched:** `src/tree/mod.rs`, `src/graph/gv_graph.rs`.

```rust
pub(crate) struct VTree<V: Accumulator> {
    pub(crate) nodes:      Arena<VNode<V>>,
    pub(crate) root:       Option<VNodeId>,
    pub(crate) violations: Vec<VNodeId>,
}
```

**Steps:**

1. Add the struct definition and a `VTree::new() -> Self` constructor.
2. In `GvGraph`, replace `vnodes`, `v_root`, and `violations` with
   `pub(crate) vtree: VTree<V>`.
3. Mechanical find-and-replace of all accesses:
   - `self.vnodes` → `self.vtree.nodes`
   - `self.v_root` → `self.vtree.root`
   - `self.violations` → `self.vtree.violations`
4. `cargo check --all-features` before the full suite.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): introduce VTree struct (fields only)`

---

## [x] Step 5.2 — Move `tree/vtree.rs` free functions → `VTree` methods

**Files touched:** `src/tree/vtree.rs`, `src/graph/algorithm/*.rs`.

| Free function                                     | New method                                            |
| ------------------------------------------------- | ----------------------------------------------------- |
| `vtree_remove_leaf(vnodes, gnodes, v_id, v_root)` | `vtree.remove_leaf(gnodes, v_id) -> Option<VNodeId>`  |
| `propagate_v_sums(vnodes, id)`                    | `vtree.propagate_sums(id)`                            |
| `sync_intensity_in_parent(vnodes, id, val)`       | `vtree.sync_intensity(id, val)`                       |
| `recompute_all_v_intensities(vnodes)`             | `vtree.recompute_all_intensities()`                   |
| `invalidate_depth_subtree(vnodes, id)`            | `vtree.invalidate_depth(id)`                          |
| `v_depth(vnodes, id)`                             | `vtree.depth(id)`                                     |
| `propagate_evictable_flags(vnodes, id)`           | `vtree.propagate_evictable(id)`                       |
| `is_ancestor(vnodes, candidate, target)`          | `vtree.is_ancestor(candidate, target)`                |
| `scan_for_candidates(graph)` (evict.rs)           | `vtree.scan_for_candidates(live_depth_evict, g_root)` |
| `scan_dfs(graph, …)` private (evict.rs)           | `vtree.scan_dfs(…)` private helper                    |

> `scan_for_candidates` and `scan_dfs` currently take `&GvGraph` but only need
> V-tree data plus two scalars from the G-tree (`live_depth_evict`, `g_root`).
> After this step they move to `VTree`; the two scalars are passed as arguments.

**Steps:**

1. Add the methods to `impl VTree<V>`.
2. Update every call site in algorithm modules.
3. Delete the old free functions once all call sites are migrated.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move vtree free functions to VTree methods`

---

## [x] Step 5.3 — Move violations queue ownership into `VTree`

**Files touched:** `src/graph/algorithm/violation_push.rs`,
`src/graph/algorithm/violation_sources.rs`, algorithm call sites.

With `violations` now a field of `VTree`, the push helpers become `VTree` methods:

- `vtree.push_violation(id: VNodeId)`
- `vtree.drain_violations() -> impl Iterator<Item = VNodeId>`

**Steps:**

1. Add the methods to `impl VTree<V>`.
2. Update `violation_push.rs` and `violation_sources.rs` to call them.
3. Update `observe.rs`, `split.rs`, `evict.rs` call sites.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move violations queue into VTree`

---

## [x] Step 5.4 — Move rebalance entry point into `VTree`

**Files touched:** `src/graph/algorithm/rebalance.rs`.

`rebalance::resolve(graph, id)` currently takes `&mut GvGraph`. Check whether it
reads any G-tree data:

- If it reads only V-tree data → move it to `vtree.rebalance(id)`.
- If it also reads G-tree data → keep it as a free function but pass G-tree data
  as a read-only argument: `rebalance::resolve(vtree, gtree_snapshot, id)`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move rebalance entry into VTree`

---

## Review checkpoint

**`GvGraph` is a clean orchestrator.** Its fields are now exactly:

```rust
pub struct GvGraph<C, V, const N> {
    pub(crate) gtree: GTree<C, V, N>,
    pub(crate) vtree: VTree<V>,
    pub(crate) config: Config<V>,

    // Phase 7 — dynamic-contour-tracking feature gate
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateaus: BTreeMap<BasisEdge<C>, Plateau<C, V>>,
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) pending_p_i4: Vec<(GNodeId, BasisEdge<C>)>,
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateau_basis: PlateauBasis<C>,
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateaus_dirty: bool,
}
```

Algorithm modules call `self.vtree.*` and `self.gtree.*` methods directly; `GvGraph`
itself exposes public accessors but does not orchestrate in a procedural sense.

**Cross-tree seams in `VTree` methods:**

Two VTree methods accept G-tree data as parameters — these are seams that would need
a test double to unit-test either tree in isolation:

| Method                                                 | G-tree argument           | Reason                                                              |
| ------------------------------------------------------ | ------------------------- | ------------------------------------------------------------------- |
| `VTree::remove_leaf(gnodes, v_id)`                     | `&mut Arena<GNode<C, V>>` | Removing a V-leaf also updates the mirrored back-pointers in GNodes |
| `VTree::scan_for_candidates(live_depth_evict, g_root)` | `GNodeId`                 | Candidates belonging to the G-root pseudo-node must be excluded     |

`rebalance::rebalance(vtree, gnodes, depth_evict)` stays as a free function because
it also needs G-tree writes; it takes `&mut VTree<V>` so the call site is clean, but
the cross-tree dependency remains explicit.

**Violations queue lifetime is obvious.** `VTree::violations` is:

- **Born** empty in `GvGraph::new()`.
- **Populated** by `push_violation(id)` inside `observe`, `evict`, and
  `violation_push::push_*` helpers (the latter receive `&mut self.vtree.violations`).
- **Drained** by `rebalance::rebalance` at the end of every `observe()`, `decay()`,
  and budget eviction pass.
- **Empty** between mutations (the `has_pending_violations()` guard on observe
  enforces this invariant at debug time).

**Remaining fields that belong in neither tree:**

The four `#[cfg(feature = "dynamic-contour-tracking")]` fields (`plateaus`,
`pending_p_i4`, `plateau_basis`, `plateaus_dirty`) are domain state that belongs to
neither the G-tree nor the V-tree. They track the geometric contour derived from the
G-tree and will be addressed in Phase 7.
