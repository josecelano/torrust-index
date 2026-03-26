# Module Reorganisation Plan

## Motivation

Several types are defined in modules where they are not strongly cohesive with
the other items in that file. The goals of this reorganisation are:

1. **Discoverability** — each file should tell its story by its name alone.
2. **Cohesion** — types that logically belong together live in the same file;
   types that are independent get their own file.
3. **Thin namespace files** — `mod.rs` files should be routing layers, not
   implementation layers.

---

## Proposals

### P1 — Extract `Config<V>` → `src/graph/config.rs` ★★★ High priority

`Config<V>` is a standalone construction-parameter type with its own
`validate()` logic. It is not part of `GvGraph`'s runtime state — it is the
_input_ used to construct one. Housing it in `graph/mod.rs` alongside the
large `GvGraph` struct obscures it.

```
src/graph/
  config.rs        ← Config<V> + impl Config<V> { fn validate }
  mod.rs           ← thin namespace; re-exports Config
```

The crate's public API (`lib.rs: pub use graph::Config`) is unchanged.

---

### P2 — Relocate `GNodeChildren` → `src/nodes/gnode.rs` ★★ Medium priority

`GNodeChildren { left: Option<GNodeId>, right: Option<GNodeId> }` is a
structural view of a `GNode`'s children. It belongs conceptually alongside
`GNode` in `src/nodes/gnode.rs`. It is `#[doc(hidden)]` and its only use is
as the return type of `GvGraph::gnode_children()`.

```
src/nodes/gnode.rs   ← add GNodeChildren (cohesive with GNode)
src/graph/mod.rs     ← remove definition; re-export via pub use
```

---

### P3 — Extract `ViolationSources` → `src/graph/algorithm/violation_sources.rs` ★ Low-medium priority

`ViolationSources` is a boolean-flags configuration struct controlling which of
the seven V-tree violation sources are checked during rebalancing. It is a
_parameter type_, not algorithm code, so it should not be buried inside the
large `rebalance.rs` file.

```
src/graph/algorithm/
  violation_sources.rs   ← ViolationSources struct + impls
  rebalance.rs           ← remove struct; use super::violation_sources::ViolationSources
```

---

### P4 — Make `graph/mod.rs` a thin namespace file (follow-on to P1–P3) ★ Low priority

After P1–P3 are applied, `graph/mod.rs` still contains the entire `GvGraph`
struct and all its `impl` blocks (~350 lines). Moving it to
`src/graph/gv_graph.rs` reduces `mod.rs` to a pure namespace/re-export layer,
mirroring the pattern used in large Rust crates such as `tokio` and `axum`.

```
src/graph/
  config.rs        ← Config<V>
  gv_graph.rs      ← GvGraph<C, V, N> struct + impl + tests + uniform_contour_depth_of
  mod.rs           ← module declarations + re-exports only
```

---

## Execution order

| Step | Action                                                     | Status  |
| ---- | ---------------------------------------------------------- | ------- |
| 1    | P1 — `Config` → `graph/config.rs`                          | ✅ Done |
| 2    | P2 — `GNodeChildren` → `nodes/gnode.rs`                    | ✅ Done |
| 3    | P3 — `ViolationSources` → `algorithm/violation_sources.rs` | ✅ Done |
| 4    | P4 — `GvGraph` → `graph/gv_graph.rs`, thin `mod.rs`        | ✅ Done |
| 5    | P5 — `Node` → `spatial/node.rs`                            | ✅ Done |
| 6    | P6 — `PlateauBasis` → `spatial/plateau_basis.rs`           | ✅ Done |

---

## Round 2: remaining modules

### P5 — Split `Node<C,V>` from `src/spatial/view.rs` → `src/spatial/node.rs` ★★ Medium priority

`view.rs` currently holds three types with different concerns:

- `Span<C,V>` — a lightweight, purely geometric half-open interval:
  `start`, `end`, `intensity`, `depth`. No G-tree knowledge.
- `Cell<C,V>` — identical shape to `Span`; represents a single terminal
  leaf cell. Converts to `Span`. Still purely geometric.
- `Node<C,V>` — a **rich structural view** of a `GNode`. Fields include
  `own`, `sum`, `GState`, `gnode_id`, `parent`. Imports `GNodeId` and
  `GState` from internal modules.

`Span` and `Cell` are cohesive: both are "narrow windows" into a coordinate
region, intended as return values from spatial queries. `Node` is conceptually
a "read-only projection of a GNode onto a user-visible type" and belongs in its
own file.

```
src/spatial/
  view.rs    ← Span<C,V> + Cell<C,V>  (purely geometric)
  node.rs    ← Node<C,V>              (structured GNode view)
```

`lib.rs` already re-exports `Node` by name so the public API is unchanged.

---

### P6 — Extract `PlateauBasis<C>` from `src/spatial/plateau.rs` → `src/spatial/plateau_basis.rs` ★★ Medium priority

`plateau.rs` currently mixes two very different concerns:

- **Public API surface**: `BasisEdge<C>` and `Plateau<C,V>` — both `pub`,
  re-exported from `lib.rs`, used by downstream callers.
- **Internal bookkeeping structure**: `PlateauBasis<C>` — `pub(crate)`,
  feature-gated (`dynamic-contour-tracking`), contains two private `HashMap`
  / `BTreeMap` internal fields and a collection of `pub(crate)` methods.

Separating them makes `plateau.rs` a single-purpose public-API file and
`plateau_basis.rs` the internal implementation file.

```
src/spatial/
  plateau.rs        ← BasisEdge<C>, Plateau<C,V>  (public API)
  plateau_basis.rs  ← PlateauBasis<C>             (internal, feature-gated)
```

---

## What is and remains NOT moved

| Type(s)                                              | Location                    | Reason                                                          |
| ---------------------------------------------------- | --------------------------- | --------------------------------------------------------------- |
| `GState`                                             | `nodes/gnode.rs`            | Tightly coupled to `GNode::state()`                             |
| `VKind`, `PackedChildren`                            | `nodes/vnode.rs`            | `VNode` implementation details                                  |
| `Pewei`, `Layer`, `Transition`, `Terminal`           | `spatial/pewei.rs`          | One serialisable snapshot format — all cohesive                 |
| `BasisElement`, `ContourRange`, `ContourRangeEnergy` | `spatial/contour_range.rs`  | One query-result type with its sub-parts                        |
| `PlateauAuditContext`, `EvictionContext`             | `diagnostics/diagnostic.rs` | Small types coupled directly to their single diagnostic fn      |
| `Nd`, `Ctx`                                          | `rebalance.rs`              | Display wrappers tightly coupled to rebalance tracing internals |
| `GNodeId`, `VNodeId`                                 | `handle.rs`                 | Share the `impl_handle!` macro; both tiny newtypes              |
| `Arena<T>`                                           | `arena.rs`                  | Standalone; single responsibility                               |
