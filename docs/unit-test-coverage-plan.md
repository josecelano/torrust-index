# Unit Test Coverage Plan

**Goal:** Reach ≥ 95% line coverage per module via idiomatic Rust TDD-style unit tests.  
Tests live inside `#[cfg(test)]` blocks at the bottom of each source file — this gives direct access to private items without `pub(crate)` annotations.

**Tooling:** `cargo llvm-cov` (see [docs/coverage-baseline.md](coverage-baseline.md) for setup)

```bash
# Run with inline coverage report
cargo llvm-cov --summary-only

# Run and open HTML report
cargo llvm-cov --open
```

---

## Baseline (post-refactor, 2026-03-24)

| Metric    | Covered | Total | Coverage   |
| --------- | ------- | ----- | ---------- |
| Lines     | 3449    | 5747  | **60.01%** |
| Functions | 255     | 444   | **57.43%** |
| Regions   | 5856    | 9523  | **61.49%** |

---

## Module Status Table

Ordered from simplest to most complex.  
✅ = done (≥ 95%) | 🔲 = not started | ⏳ = in progress

| #   | Module                         | Current | Target  | Status |
| --- | ------------------------------ | ------- | ------- | ------ |
| 1   | `traits/coordinate.rs`         | 22%     | 95%     | ✅     |
| 2   | `traits/accumulator.rs`        | 16%     | 95%     | ✅     |
| 3   | `traits/attenuatable.rs`       | 20%     | 95%     | ✅     |
| 4   | `traits/inspectable.rs`        | 16%     | 95%     | ✅     |
| 5   | `traits/observation.rs`        | 14%     | 95%     | ✅     |
| 6   | `traits/proratable.rs`         | 0%      | 95%     | ✅     |
| 7   | `traits/weighable.rs`          | 33%     | 95%     | ✅     |
| 8   | `traits/rng.rs`                | 0%      | 95%     | ✅     |
| 9   | `handle.rs`                    | 100%    | 100%    | ✅     |
| 10  | `arena.rs`                     | 78%     | 95%     | ✅     |
| 11  | `nodes/gnode.rs`               | 92%     | 95%     | ✅     |
| 12  | `nodes/vnode.rs`               | 82%     | 95%     | ✅     |
| 13  | `spatial/view.rs`              | 0%      | 95%     | ✅     |
| 14  | `spatial/plateau.rs`           | 78%     | 95%     | ✅     |
| 15  | `spatial/pewei.rs`             | 0%      | 95%     | ✅     |
| 16  | `spatial/contour_range.rs`     | 0%      | 95%\*   | ⏳     |
| 17  | `tree/gtree.rs`                | 100%    | 100%    | ✅     |
| 18  | `tree/vtree.rs`                | 91%     | 95%     | ✅     |
| 19  | `graph/mod.rs`                 | 88%     | 95%     | ✅     |
| 20  | `graph/traits.rs`              | 0%      | 95%     | ✅     |
| 21  | `graph/algorithm/extract.rs`   | 31%     | 95%     | ✅     |
| 22  | `graph/algorithm/query.rs`     | 27%     | 95%     | ✅     |
| 23  | `graph/algorithm/decay.rs`     | 52%     | 95%     | ✅     |
| 24  | `graph/algorithm/observe.rs`   | 87%     | 95%     | ✅     |
| 25  | `graph/algorithm/split.rs`     | 96%     | 100%    | ✅     |
| 26  | `graph/algorithm/evict.rs`     | 73%     | 95%     | ✅     |
| 27  | `graph/algorithm/budget.rs`    | 80%     | 95%     | ✅     |
| 28  | `graph/algorithm/rebalance.rs` | 70%     | 95%     | ✅     |
| 29  | `graph/algorithm/plateau.rs`   | 66%     | 95%     | ✅     |
| 30  | `diagnostics/invariants.rs`    | 57%     | 80%\*\* | ✅     |
| 31  | `diagnostics/diagnostic.rs`    | 5%      | 80%\*\* | ✅     |

\* `contour_range.rs` is only active under `--features dynamic-contour-tracking`  
\*\* `invariants.rs` and `diagnostic.rs` contain many branches that fire only when the data structure is in an _invalid_ state — deliberately creating invalid states is difficult without breaking the abstraction. 80% is realistic.

---

## Conventions

All tests live in the source file they test, in a `#[cfg(test)] mod tests` block at the bottom.
When multiple concrete impls exist (e.g. `u8`, `u32`, `f32`, `f64`) use nested submodules to group by impl type.
This keeps individual test names short and makes the output tree readable in `cargo test`.

```rust
// at the bottom of every .rs file
#[cfg(test)]
mod tests {
    // one nested mod per concrete type / logical grouping
    mod u32 {
        use crate::traits::SomeTrait;

        #[test]
        fn zero_returns_zero() { ... }

        #[test]
        fn add_sub_round_trip() { ... }
    }

    mod f64 {
        use crate::traits::SomeTrait;

        #[test]
        fn zero_returns_zero() { ... }
    }
}
```

Test names use plain `snake_case` within their submodule — the submodule provides the grouping context.
Each test has exactly one assertion (or logically grouped assertions for the same behaviour).
No mocks — use the real types at the smallest scope possible.

---

## Execution plan

Work through modules in order. After each module:

1. Write tests inside the source file's `#[cfg(test)] mod tests { … }` block
2. Run `cargo llvm-cov --summary-only` to verify coverage
3. Commit with `test(<module>): add unit tests`
4. Update the Status column above

---

## Per-module test sketches

### 1. `traits/coordinate.rs`

The trait has default methods implemented for `u8`, `u16`, `u32`, `u64` via the blanket impl.  
Tests should use `u8` (tiny domain) to keep values readable.

Key scenarios:

- `zero()` returns 0
- `domain_max(n)` returns `(1 << n) - 1`
- `midpoint(0, 256)` returns 128 for `u32`
- `width(a, b)` equals `b - a`
- `is_final` is true at max depth
- `from_u64` / `to_f64` round-trip
- `next_value` is saturating at max

### 2. `traits/accumulator.rs`

Tests for `u32`, `u64`:

- `zero()` returns 0
- `add(a, b)` is commutative
- `sub(a, b)` == `a - b`
- `add(zero(), x) == x`

### 3–8. Remaining traits

Each trait: construct one concrete impl (or use built-in impls), call every method, assert the return value or side-effect. Covers the default blanket impls.

### 10. `arena.rs`

- new arena is empty (`count == 0`, no occupied slots)
- `alloc` increases count
- `get` returns the stored value
- `dealloc` decreases count and returns the value
- `alloc` after `dealloc` reuses the freed slot (same index)
- `is_occupied` is false after dealloc
- `iter_occupied` yields exactly the allocated indices

### 11. `nodes/gnode.rs`

- A fresh terminal node has no children
- `state()` returns `Terminal` / `Internal` / `SemiInternal` correctly
- `uncovered_range()` returns `None` when both children exist
- Uncovered range is returned when only left child is present

### 12. `nodes/vnode.rs`

- `PackedChildren::new_2` has len 2
- `heaviest_child_index` returns the index of the max-weight child
- `find_index` returns `None` for unknown id
- `add_child` promotes 2-children to 3-children
- `remove_child` demotes 3-children to 2-children
- `update_intensity` changes the weight at the given index

### 13. `spatial/view.rs`

- `Span::width()` == `end - start`
- `Node::refinement()` == `sum - own`
- `Node::is_root()` true when no parent
- `Node::to_span()` preserves range and sum
- `Node::to_cell()` returns `Some` iff terminal

### 14. `spatial/plateau.rs`

- `PlateauBasis::new()` is empty (count 0)
- `insert` then `plateau_key` round-trip
- `remove` on absent key returns `None`
- `plateau_count` reflects distinct BasisEdge values
- `basis_count` reflects total membership count

### 15. `spatial/pewei.rs`

`Pewei` is produced by `GvGraph::extract()`. Tests here build one via `extract` and assert:

- `layer_count()` matches depth of the tree
- `total_energy()` equals the graph's `total_sum`
- `reconstruct(max_layer)` spans cover the full 0..domain_max range
- Transition `snr()` is `Some` and `> 0`

### 16. `spatial/contour_range.rs`

Requires `--features dynamic-contour-tracking`.

- `validate_endpoints` returns `None` for out-of-domain range
- `compute_plateau_energy` sums the correct nodes
- `ContourRange` total energy matches graph query

### 18. `tree/vtree.rs`

- `v_depth` returns 0 for the root
- `propagate_v_sums` corrects parent after child update
- `invalidate_depth_subtree` marks depth stale

### 19. `graph/mod.rs`

- `Config::default()` constructs without panic
- `GvGraph::new(config)` starts with zero sum
- `gnode_children()` returns both children after a split
- `g_root()` is stable after observe calls
- Accessor `total_sum()` matches accumulated observations

### 20. `graph/traits.rs`

Exercise the four trait impls via the trait objects:

- `SpatialWrite::observe` increases total_sum
- `SpatialRead::get` returns the observed value
- `TemporalDecay::decay` reduces total_sum
- `WeightedSampler::sample` on empty graph returns `None`

### 21. `graph/algorithm/extract.rs`

- `extract()` on a single-observation graph returns one terminal
- `layers()` visits all active nodes exactly once
- `from_observations(config, iter)` builds the same graph as sequential observe calls
- `Extend` inserts all observations

### 22. `graph/algorithm/query.rs`

- `get(coord)` returns zero for unobserved coordinate
- `sample` returns `Some` after observations
- `range_sum(0..domain_max)` equals `total_sum`
- `range_sum` for a sub-range is ≤ `total_sum`
- `contour_range` returns `None` for invalid range

### 23. `graph/algorithm/decay.rs`

- Uniform decay halves total_sum when attenuation = 0.5
- Selective decay leaves shallow nodes more than deep nodes untouched (relative ordering check)
- Decay with attenuation = 1.0 drives sum to zero

### 24. `graph/algorithm/observe.rs`

Already 87%. Add:

- Observer-triggered split: after `split_threshold + 1` observations same coord, tree grows

### 26. `graph/algorithm/evict.rs`

- After `depth_evict` exceeded, tip nodes are removed
- `total_sum` is preserved after eviction (values promoted up)

### 27. `graph/algorithm/budget.rs`

- After many observations, depth is bounded by `depth_evict`
- Budget-bound apply_budget reduces node count

### 28. `graph/algorithm/rebalance.rs`

- A valid tree has no violations
- After deliberate insertion of unbalanced sum, rebalance resolves violations

### 29. `graph/algorithm/plateau.rs`

- After observe(), the plateau neighbours of the split point shift correctly
- Plateau count does not grow unboundedly with observation count

### 30. `diagnostics/invariants.rs`

- `check_all_invariants` returns empty vec for a freshly constructed graph
- After every `observe` call, invariants still hold (property: invariants hold after mutation)
- Note: branches triggered by _invalid_ states are unreachable from valid inputs; document expected uncoverable branches per-function

### 31. `diagnostics/diagnostic.rs`

- `audit_violations` returns no output on a clean graph
- Note: most helper functions only activate in debug builds (`#[cfg(debug_assertions)]`) — document those as `#[allow(dead_code)]` in release builds and consider them acceptable 0-coverage lines
