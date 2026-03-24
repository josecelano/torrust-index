# Refactor Proposal: Restructure Module Layout

**Status:** Proposed — blocked on test coverage  
**Prerequisite:** Adequate test suite before executing (the restructure touches every `use crate::` path in the codebase)

---

## Problem

The current flat `src/` layout places ~20 files at the same depth regardless of their abstraction level. This creates cognitive load:

- No visual distinction between entry points, primitives, and internal algorithms
- `graph_budget`, `graph_plateau`, `graph_query`, `graph_extract`, `graph_traits` use the module name as a namespace prefix — exactly what Rust subdirectories are for
- A newcomer cannot tell what to open first

---

## Proposed Structure

```
src/
  lib.rs                      ← public re-exports only (unchanged)
  arena.rs                    ← ① Foundation (widely referenced, stays at root)
  handle.rs                   ← ① Foundation (widely referenced, stays at root)
  traits/                     ← ① Foundation traits (already a subdirectory — unchanged)

  nodes/                      ← ② Node types
    mod.rs
    gnode.rs                  ← was src/gnode.rs
    vnode.rs                  ← was src/vnode.rs

  spatial/                    ← ③ Spatial / output types
    mod.rs
    view.rs                   ← was src/view.rs
    plateau.rs                ← was src/plateau.rs
    pewei.rs                  ← was src/pewei.rs
    contour_range.rs          ← was src/contour_range.rs

  tree/                       ← ④ Tree utilities
    mod.rs
    gtree.rs                  ← was src/gtree.rs
    vtree.rs                  ← was src/vtree.rs

  graph/                      ← ⑤ Core data structure + public API
    mod.rs                    ← was src/graph.rs  (GvGraph, Config, accessors)
    traits.rs                 ← was src/graph_traits.rs
    algorithm/                ← ⑥ All mutations and queries
      mod.rs
      observe.rs              ← was src/observe.rs   (orchestrator / write entry point)
      split.rs                ← was src/split.rs
      rebalance.rs            ← was src/rebalance.rs
      evict.rs                ← was src/evict.rs
      decay.rs                ← was src/decay.rs
      budget.rs               ← was src/graph_budget.rs
      plateau.rs              ← was src/graph_plateau.rs
      query.rs                ← was src/graph_query.rs
      extract.rs              ← was src/graph_extract.rs

  diagnostics/                ← ⑧ Read-only invariant checks
    mod.rs
    invariants.rs             ← was src/invariants.rs
    diagnostic.rs             ← was src/diagnostic.rs
```

---

## Key Benefits

| Before                 | After                                                  |
| ---------------------- | ------------------------------------------------------ |
| `graph_budget`         | `graph::algorithm::budget` — prefix is the module path |
| `graph_traits`         | `graph::traits`                                        |
| 20 files at root depth | 5 top-level entries, depth signals abstraction level   |
| No obvious entry point | `graph/mod.rs` is clearly the centre                   |

---

## What Changes

- All `use crate::` paths inside every file
- One `mod.rs` per new directory (5 files)
- `lib.rs` module declarations (the public `pub use` re-exports stay identical)
- The `component-architecture.puml` diagram needs updating to reflect new paths

## What Does NOT Change

- Public API (`pub use` items in `lib.rs`)
- Logic inside any file
- `Cargo.toml`

---

## Execution Plan

1. Add tests sufficient to catch import/visibility regressions
2. Move files with `git mv` (preserves history)
3. Create `mod.rs` for each new directory
4. Update all `use crate::` paths (mechanical — can be done with `sed` + compiler errors as guide)
5. Run `cargo test` and `cargo doc` to verify
6. Update `docs/component-architecture.puml` and regenerate SVGs
