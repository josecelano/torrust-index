# Phase 4 — Introduce `GTree<C, V, N>`

**Opportunities:** [#1](../design-improvement-opportunities.md#1-gvgraph-is-a-god-object),
[#2](../design-improvement-opportunities.md#2-the-two-trees-have-no-explicit-types)

**Prerequisite:** Phase 3 complete (node fields encapsulated, reduces noise in this phase).

**Goal:** Give the G-tree a proper type that owns its arena, root, and operational
counters, and expose tree operations as methods rather than free functions.

---

## [x] Step 4.1 — Define `GTree` as a struct wrapper (no logic yet)

**Files touched:** `src/tree/mod.rs` (or new `src/tree/gtree_type.rs`),
`src/graph/gv_graph.rs`.

```rust
pub(crate) struct GTree<C: Coordinate, V: Accumulator, const N: u32> {
    pub(crate) nodes:              Arena<GNode<C, V>>,
    pub(crate) root:               GNodeId,
    pub(crate) node_count:         u32,
    pub(crate) terminal_count:     u32,
    pub(crate) live_depth_evict:   u32,
    pub(crate) live_depth_create:  u32,
    pub(crate) depth_buffer:       u32,
    pub(crate) headroom:           usize,
    pub(crate) soft_limit:         Option<usize>,
}
```

**Steps:**

1. Add the struct definition and a `GTree::new(config: &StructuralConfig) -> Self`
   constructor (moves the `Capacity` computation out of `GvGraph::new`).
2. In `GvGraph`, replace the corresponding fields with `pub(crate) gtree: GTree<C,V,N>`.
3. Do a mechanical find-and-replace of all accesses:
   - `self.gnodes` → `self.gtree.nodes`
   - `self.g_root` → `self.gtree.root`
   - `self.node_count` → `self.gtree.node_count`
   - etc.
4. `cargo check --all-features` before running the full suite.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): introduce GTree struct (fields only)`

---

## [x] Step 4.2 — Move `tree/gtree.rs` free functions → `GTree` methods

**Files touched:** `src/tree/gtree.rs`, `src/graph/algorithm/*.rs`.

| Free function                                | New method                                                |
| -------------------------------------------- | --------------------------------------------------------- |
| `route_to_receiver(gnodes, root, x)`         | `gtree.route_to(x) -> GNodeId`                            |
| `gnode_depth_from_interval(lo, hi, n)`       | `GTree::depth_of_interval(lo, hi) -> u32` (associated fn) |
| `recompute_g_sums(gnodes, start)`            | `gtree.recompute_sums(start)`                             |
| `recompute_g_sums_subtree(gnodes, preorder)` | `gtree.recompute_sums_subtree(preorder)`                  |
| `uniform_contour_depth_of(gnodes, gid, n)`   | `gtree.uniform_contour_depth(gid) -> Option<u32>`         |

**Steps:**

1. Add the methods to `impl GTree<C,V,N>`.
2. Update every call site in algorithm modules to use the method form.
3. Mark the old free functions `#[deprecated]` / `#[allow(dead_code)]` temporarily.
4. Delete the old free functions once all call sites are migrated.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move gtree free functions to GTree methods`

---

## [x] Step 4.3 — Convert `attempt_split` / `evict_tip` to `GvGraph` methods; extract G-tree-only helpers into `GTree`

**Files touched:** `src/graph/algorithm/split.rs`, `src/graph/algorithm/evict.rs`,
`src/tree/gtree.rs` (as new methods), `src/graph/gv_graph.rs`.

`attempt_split(graph, g_id)` and `evict_tip(graph, v_id)` both take `&mut GvGraph`
and coordinate across both trees — they are cross-tree orchestrators. They must not
become `GTree`/`VTree` methods; instead convert them to `GvGraph` methods:

```rust
impl<C, V, const N> GvGraph<C, V, N> {
    pub(crate) fn attempt_split(&mut self, g_id: GNodeId) { … }
    pub(crate) fn evict_tip(&mut self, v_id: VNodeId)    { … }
}
```

Then extract the sub-operations that **only** touch G-tree fields (node allocation,
parent linking, sum recomputation) into dedicated `GTree` methods called from inside
those orchestrators:

- `gtree.allocate_children(parent_id, …) -> (GNodeId, GNodeId)`
- `gtree.merge_into_parent(child_id)` — absorbs own weight, deallocates node

> Only move code that does **not** also read or write V-tree state into `GTree`.
> Cross-tree coordination stays in the `GvGraph` method body. When in doubt,
> leave it in `GvGraph` and document it as a cross-tree seam.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(graph): convert attempt_split/evict_tip to GvGraph methods; extract GTree helpers`

---

## Review checkpoint

> _Fill in after completing all three steps._
>
> - Did any free function turn out to need both `gnodes` and `vnodes`? Document
>   it as a cross-tree operation and leave it on `GvGraph` or a future coordinator.
> - Is the `Capacity` computation now cleanly inside `GTree::new()`? Or does
>   `GvGraph::new()` still duplicate any of it?
> - Are `node_count` and `terminal_count` correctly maintained through the
>   `GTree` method boundary, or are there callers that still update them directly?
> - After this phase: what fields remain in `GvGraph` that are not `gtree`,
>   `vtree` (Phase 5), `config`, or plateau state (Phase 7)?
