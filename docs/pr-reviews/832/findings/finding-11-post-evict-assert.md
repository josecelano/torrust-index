# Finding #11 — Post-Evict `debug_assert` Fires in Debug Builds

**Status:** ❗ CONFIRMED  
**Severity:** Crash (debug builds) · Test-coverage blocker  
**File:** `packages/mudlark/src/graph_plateau.rs` (`plateau.rs` in refactored layout), line ~127  
**Discovered:** 2026-03-25 during isolated test-coverage analysis  
**Branch:** `review/pr-832-mudlark-isolated`

---

## Description

A `debug_assert!` in the plateau-maintenance code fires immediately after any
eviction in debug builds:

```
"POST-EVICT-SINGLE-1: dynamic-contour-tracking mirror diverged"
```

This assertion checks that the dynamic contour-tracking mirror (maintained
incrementally) still matches a freshly computed reference after the eviction
path. The mirror diverges during eviction, triggering the assert.

Because the assert fires in every debug test that triggers eviction, **all
eviction-gated code paths are unreachable in `cargo test` (debug mode).**

---

## Impact on Test Coverage

The following modules have coverage ceilings caused entirely by this assertion:

| Module                         | Line Cov. | Gap cause                                    |
| ------------------------------ | --------- | -------------------------------------------- |
| `graph/algorithm/plateau.rs`   | 72%       | eviction-gated plateau merge/split paths     |
| `graph/algorithm/evict.rs`     | 77%       | full eviction flow                           |
| `diagnostics/invariants.rs`    | 77%       | SemiInternal invariant checks post-eviction  |
| `graph/algorithm/rebalance.rs` | 79%       | contract/legacy-promote paths after eviction |
| `graph/algorithm/query.rs`     | 84%       | SemiInternal `decompose_basis` paths         |
| `diagnostics/diagnostic.rs`    | 85%       | `tracing::error!/debug!` lazy format args    |

Overall line coverage ceiling: **~87%** with this bug present.

---

## Distinction from Finding #9

| Aspect  | Finding #9                                  | Finding #11                                           |
| ------- | ------------------------------------------- | ----------------------------------------------------- |
| Assert  | `assert_eq!` on f64 plateau sum after split | `debug_assert!` on contour-tracking mirror post-evict |
| Trigger | G-node **split** with f64 values            | G-node **eviction** (any value type)                  |
| Release | Silent wrong data                           | Assert disabled → hidden mirror divergence            |
| Impact  | Wrong data in release, crash in debug       | Test-coverage blocker in debug                        |

---

## Reproduction

Write a test that observes enough traffic to exhaust the node budget and trigger
eviction, with `dynamic-contour-tracking` enabled (the default):

```rust
#[test]
fn eviction_is_reachable() {
    use torrust_mudlark::{GvGraph, Config};
    let mut graph: GvGraph<u32, u64, 8> = GvGraph::new(Config {
        node_budget: 10,
        split_threshold: 2,
        ..Default::default()
    });
    // Exhaust budget to force eviction
    for coord in 0_u32..200 {
        graph.observe(coord, 1);
    }
    // debug_assert fires here → test panics in debug, passes in release
}
```

Run with `cargo test` (debug) → panic. Run with `cargo test --release` → passes.

---

## Recommended Fix

Investigate whether the mirror divergence is a genuine logic error or a benign
ordering artefact. If benign: relax the `debug_assert!` to a `tracing::warn!`
and add a comment. If genuine: fix the eviction logic and update the assertion.

Until resolved, the mutation kill rate re-run (see Open Items) should be
performed in `--release` mode so that eviction mutants are actually tested.
