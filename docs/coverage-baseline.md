# Test Coverage Baseline

Generated with:

```bash
cargo llvm-cov
```

Tool: `cargo-llvm-cov 0.6.16`  
Date: 2026-03-24  
Tests: 10 integration tests in `tests/integration.rs`

---

## Summary

| Metric    | Covered | Total | Coverage   |
| --------- | ------- | ----- | ---------- |
| Regions   | 5765    | 9513  | **60.60%** |
| Functions | 251     | 444   | **56.53%** |
| Lines     | 3288    | 5522  | **59.54%** |

---

## Per-file Coverage

| File                     | Lines | Missed | Line Cover | Functions | Missed Fn |  Fn Cover |
| ------------------------ | ----: | -----: | ---------: | --------: | --------: | --------: |
| `arena.rs`               |    84 |     18 |     78.57% |        14 |         3 |    78.57% |
| `contour_range.rs`       |    68 |     68 |  **0.00%** |         5 |         5 | **0.00%** |
| `decay.rs`               |   168 |     80 |     52.38% |         6 |         3 |    50.00% |
| `diagnostic.rs`          |   175 |    164 |  **6.29%** |        11 |        10 | **9.09%** |
| `evict.rs`               |   365 |     96 |     73.70% |        15 |         4 |    73.33% |
| `gnode.rs`               |    38 |      3 |     92.11% |         6 |         1 |    83.33% |
| `graph.rs`               |   199 |     47 |     76.38% |        25 |         7 |    72.00% |
| `graph_budget.rs`        |    95 |     19 |     80.00% |         5 |         2 |    60.00% |
| `graph_extract.rs`       |   101 |    101 |  **0.00%** |         5 |         5 | **0.00%** |
| `graph_plateau.rs`       |   861 |    279 |     67.60% |        50 |        15 |    70.00% |
| `graph_query.rs`         |   236 |    172 |     27.12% |        16 |         9 |    43.75% |
| `graph_traits.rs`        |    17 |     17 |  **0.00%** |         5 |         5 | **0.00%** |
| `gtree.rs`               |    58 |      0 |    100.00% |         8 |         0 |   100.00% |
| `handle.rs`              |    12 |      0 |    100.00% |         3 |         0 |   100.00% |
| `invariants.rs`          |  1029 |    435 |     57.73% |        70 |        27 |    61.43% |
| `observe.rs`             |    69 |      9 |     86.96% |         1 |         0 |   100.00% |
| `pewei.rs`               |   197 |    197 |  **0.00%** |        18 |        18 | **0.00%** |
| `plateau.rs`             |    88 |     19 |     78.41% |        18 |         4 |    77.78% |
| `rebalance.rs`           |   838 |    248 |     70.41% |        45 |         9 |    80.00% |
| `split.rs`               |   244 |      9 |     96.31% |         6 |         0 |   100.00% |
| `traits/accumulator.rs`  |    25 |     21 |     16.00% |         9 |         7 |    22.22% |
| `traits/attenuatable.rs` |    15 |     12 |     20.00% |         3 |         2 |    33.33% |
| `traits/coordinate.rs`   |    88 |     68 |     22.73% |        30 |        23 |    23.33% |
| `traits/inspectable.rs`  |    18 |     15 |     16.67% |         6 |         5 |    16.67% |
| `traits/observation.rs`  |    21 |     18 |     14.29% |         7 |         6 |    14.29% |
| `traits/proratable.rs`   |    25 |     25 |  **0.00%** |         6 |         6 | **0.00%** |
| `traits/rng.rs`          |     3 |      3 |  **0.00%** |         1 |         1 | **0.00%** |
| `traits/weighable.rs`    |     9 |      6 |     33.33% |         3 |         2 |    33.33% |
| `view.rs`                |    47 |     47 |  **0.00%** |        10 |        10 | **0.00%** |
| `vnode.rs`               |   100 |     18 |     82.00% |        17 |         3 |    82.35% |
| `vtree.rs`               |   229 |     20 |     91.27% |        20 |         1 |    95.00% |

---

## Analysis

### Well-covered core (≥ 80%)

These modules are exercised thoroughly by the current 10 integration tests:

- `gtree.rs` — 100% (G-tree routing used by every `observe` and `get`)
- `handle.rs` — 100%
- `split.rs` — 96% (triggered by repeated observations)
- `gnode.rs` — 92%
- `vtree.rs` — 91%
- `observe.rs` — 87% (main mutation entry point)
- `graph_budget.rs` — 80%
- `rebalance.rs` — 70%

### Partially covered (30–80%)

- `graph_plateau.rs` — 68% (plateau maintenance; needs more diverse observation patterns)
- `invariants.rs` — 58% (many invariant checks not triggered because those violations don't occur with valid inputs)
- `decay.rs` — 52% (only uniform decay path covered; selective decay uncovered)
- `graph_query.rs` — 27% (contour-range query path not exercised)

### Zero coverage — actionable gaps

These files have **0% line coverage** — they are either untested or feature-gated:

| File                   | Reason / Next step                                              |
| ---------------------- | --------------------------------------------------------------- |
| `contour_range.rs`     | Plateau/contour tracking (`dynamic-contour-tracking` feature)   |
| `graph_extract.rs`     | `extract()` and `reconstruct()` — not called by any test        |
| `graph_traits.rs`      | Trait impls not used (tests call methods directly on `GvGraph`) |
| `pewei.rs`             | `Pewei` output type — only produced by `graph_extract.rs`       |
| `view.rs`              | Output types — not constructed directly in tests                |
| `traits/proratable.rs` | Unused trait in current tests                                   |
| `traits/rng.rs`        | `rand` feature impl not called directly                         |

### Low coverage traits (< 35%)

The `traits/` files have low numbers because macros expand many impls (`u8`, `u16`, `u32`, etc.) but tests only use `u32`+`u64`. This is expected — not a priority.

---

## Coverage Improvement Goals (pre-refactor)

The current **~60% line coverage** is enough to proceed with the module restructure safely, because:

1. The zero-coverage files are either output types (`view`, `pewei`) or feature-gated (`contour_range`)
2. The core mutation path (`observe` → `split` → `rebalance` → `evict` → `budget`) is well-covered
3. `check_all_invariants()` acts as a structural oracle even for indirectly covered code

**To reach 80%+ before the refactor**, the highest-leverage additions are:

- [ ] Add a test calling `graph_traits` methods via the trait objects (`SpatialRead`, `SpatialWrite`, etc.)
- [ ] Add a test calling `graph.extract()` to cover `graph_extract.rs` and `pewei.rs`
- [ ] Add a test that reads `view::Cell` and `view::Node` fields to cover `view.rs`
- [ ] Add a selective-decay test to cover the second branch in `decay.rs`

---

## Regenerate

```bash
cargo llvm-cov > docs/coverage-baseline.txt 2>&1
```

Or for HTML output (browsable, highlights uncovered lines):

```bash
cargo llvm-cov --html --output-dir docs/coverage-html
```

> Note: add `docs/coverage-html/` to `.gitignore` — HTML report is large and auto-generated.
