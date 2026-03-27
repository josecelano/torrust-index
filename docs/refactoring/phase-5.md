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

## [ ] Step 5.3 — Move violations queue ownership into `VTree`

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

## [ ] Step 5.4 — Move rebalance entry point into `VTree`

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

> _Fill in after completing all four steps._
>
> - After Phase 4 and 5, `GvGraph` should hold: `gtree: GTree`, `vtree: VTree`,
>   `config: Config<V>`, and the plateau state (to be tackled in Phase 7). Does it
>   feel like a clean orchestrator?
> - Did any V-tree method need to reach into the G-tree? List those cross-tree
>   calls here — they are the seams that would need a test double for unit testing
>   either tree in isolation.
> - Is the violations queue lifetime now obvious? (Born when `GvGraph` is created,
>   drained at the end of every `observe()` call, empty between mutations.)
> - Are there remaining fields in `GvGraph` that belong in neither tree?
