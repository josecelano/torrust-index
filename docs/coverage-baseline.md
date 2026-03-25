# Test Coverage Baseline

Generated with:

```bash
cargo llvm-cov
```

Tool: `cargo-llvm-cov 0.6.16`  
Date: 2026-03-25  
Tests: 436 total (422 unit tests in `src/`, 10 integration tests in `tests/integration.rs`, 4 snapshot tests in `tests/snapshot_tests.rs`)

---

## Summary

| Metric    | Covered | Total | Coverage   |
| --------- | ------- | ----: | ---------- |
| Regions   | 12005   | 13796 | **87.02%** |
| Functions | 813     |   841 | **96.67%** |
| Lines     | 7249    |  8336 | **86.96%** |

---

## Per-file Coverage

| File                           | Lines | Missed | Line Cover | Functions | Missed Fn | Fn Cover |
| ------------------------------ | ----: | -----: | ---------: | --------: | --------: | -------: |
| `arena.rs`                     |   192 |      0 |    100.00% |        32 |         0 |  100.00% |
| `diagnostics/diagnostic.rs`    |   357 |     54 |     84.87% |        27 |         1 |   96.30% |
| `diagnostics/invariants.rs`    |  1223 |    287 |     76.53% |        92 |         4 |   95.65% |
| `graph/algorithm/budget.rs`    |   158 |     19 |     87.97% |        13 |         1 |   92.31% |
| `graph/algorithm/decay.rs`     |   283 |      4 |     98.59% |        23 |         0 |  100.00% |
| `graph/algorithm/evict.rs`     |   419 |     96 |     77.09% |        22 |         4 |   81.82% |
| `graph/algorithm/extract.rs`   |   182 |      5 |     97.25% |        17 |         0 |  100.00% |
| `graph/algorithm/observe.rs`   |   146 |     11 |     92.47% |        10 |         0 |  100.00% |
| `graph/algorithm/plateau.rs`   |  1029 |    290 |     71.82% |        62 |        12 |   80.65% |
| `graph/algorithm/query.rs`     |   431 |     68 |     84.22% |        42 |         1 |   97.62% |
| `graph/algorithm/rebalance.rs` |   979 |    209 |     78.65% |        62 |         5 |   91.94% |
| `graph/algorithm/split.rs`     |   311 |     16 |     94.86% |        15 |         0 |  100.00% |
| `graph/mod.rs`                 |   338 |      0 |    100.00% |        49 |         0 |  100.00% |
| `graph/traits.rs`              |    94 |      0 |    100.00% |        18 |         0 |  100.00% |
| `handle.rs`                    |    12 |      0 |    100.00% |         3 |         0 |  100.00% |
| `nodes/gnode.rs`               |   126 |      0 |    100.00% |        25 |         0 |  100.00% |
| `nodes/vnode.rs`               |   200 |      0 |    100.00% |        37 |         0 |  100.00% |
| `spatial/contour_range.rs`     |    72 |      9 |     87.50% |         5 |         0 |  100.00% |
| `spatial/pewei.rs`             |   525 |      9 |     98.29% |        38 |         0 |  100.00% |
| `spatial/plateau.rs`           |   214 |      2 |     99.07% |        39 |         0 |  100.00% |
| `spatial/view.rs`              |   126 |      0 |    100.00% |        24 |         0 |  100.00% |
| `traits/accumulator.rs`        |    68 |      0 |    100.00% |        21 |         0 |  100.00% |
| `traits/attenuatable.rs`       |    36 |      0 |    100.00% |        10 |         0 |  100.00% |
| `traits/coordinate.rs`         |   193 |      0 |    100.00% |        63 |         0 |  100.00% |
| `traits/inspectable.rs`        |    43 |      0 |    100.00% |        13 |         0 |  100.00% |
| `traits/observation.rs`        |    36 |      0 |    100.00% |        11 |         0 |  100.00% |
| `traits/proratable.rs`         |    48 |      0 |    100.00% |        13 |         0 |  100.00% |
| `traits/rng.rs`                |    44 |      0 |    100.00% |        10 |         0 |  100.00% |
| `traits/weighable.rs`          |    17 |      0 |    100.00% |         5 |         0 |  100.00% |
| `tree/gtree.rs`                |    58 |      0 |    100.00% |         8 |         0 |  100.00% |
| `tree/vtree.rs`                |   376 |      8 |     97.87% |        32 |         0 |  100.00% |

---

## Analysis

### 100% covered (lines)

- `arena.rs` — allocation, deallocation, Clone, Debug, Default all exercised
- `graph/mod.rs` — public API fully exercised
- `graph/traits.rs`
- `handle.rs`
- `nodes/gnode.rs` — all states, default, has_dependents
- `nodes/vnode.rs` — PackedChildren, VNode clone/default
- `spatial/view.rs` — output types fully covered
- `traits/accumulator.rs`, `traits/attenuatable.rs`, `traits/coordinate.rs`
- `traits/inspectable.rs`, `traits/observation.rs`, `traits/proratable.rs`
- `traits/rng.rs`, `traits/weighable.rs`
- `tree/gtree.rs`

### Well-covered (≥ 87%)

- `graph/algorithm/decay.rs` — 99%
- `spatial/plateau.rs` — 99%
- `spatial/pewei.rs` — 98%
- `tree/vtree.rs` — 98%
- `graph/algorithm/extract.rs` — 97%
- `graph/algorithm/split.rs` — 95%
- `graph/algorithm/observe.rs` — 92%
- `spatial/contour_range.rs` — 88%
- `graph/algorithm/budget.rs` — 88%

### Partially covered (50–87%)

- `diagnostics/diagnostic.rs` — 85% (tracing-based diagnostic paths not exercised without a subscriber)
- `graph/algorithm/query.rs` — 84% (SemiInternal gnode paths blocked by eviction bug)
- `graph/algorithm/rebalance.rs` — 79% (eviction-triggered rebalance paths not reachable)
- `graph/algorithm/evict.rs` — 77% (eviction blocked by `debug_assert` in `plateau.rs:127`)
- `diagnostics/invariants.rs` — 77% (violation-detection branches need invalid-state inputs; SemiInternal paths blocked)
- `graph/algorithm/plateau.rs` — 72% (plateau maintenance paths gated behind eviction)

### Remaining gaps

| File                           | Gaps                                                                                           |
| ------------------------------ | ---------------------------------------------------------------------------------------------- |
| `graph/algorithm/plateau.rs`   | 72% — eviction-gated plateau merge/split paths (blocked by `debug_assert` in `plateau.rs:127`) |
| `diagnostics/invariants.rs`    | 77% — SemiInternal invariant checks require eviction; violation branches need corrupted state  |
| `graph/algorithm/evict.rs`     | 77% — full eviction flow blocked by post-evict `debug_assert` in `plateau.rs:127`              |
| `graph/algorithm/rebalance.rs` | 79% — contract/legacy-promote paths triggered only after eviction                              |
| `graph/algorithm/query.rs`     | 84% — SemiInternal decompose-basis paths blocked by eviction                                   |
| `diagnostics/diagnostic.rs`    | 85% — `tracing::error!/debug!` format args are lazy; not evaluated without a subscriber        |

> **Root cause of remaining gaps:** A `debug_assert` in `plateau.rs:127` ("POST-EVICT-SINGLE-1:
> dynamic-contour-tracking mirror diverged") fires after eviction, blocking all test paths that
> require eviction. Once this is resolved, the eviction-gated paths in `evict.rs`, `plateau.rs`,
> `rebalance.rs`, `query.rs`, and `invariants.rs` can be covered.

---

## Regenerate

```bash
cargo llvm-cov
```

Or for HTML output (browsable, highlights uncovered lines):

```bash
cargo llvm-cov --html --output-dir docs/coverage-html
```

> Note: add `docs/coverage-html/` to `.gitignore` — HTML report is large and auto-generated.
