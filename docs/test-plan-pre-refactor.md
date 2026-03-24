# Test Plan: Minimum Safety Net Before Module Restructure

**Goal:** Establish integration tests that exercise the public API end-to-end and assert
structural correctness via the built-in invariant checker. These tests must:

1. Compile and pass on the current code
2. Remain unchanged through the module restructure (they only use `pub use` re-exports)
3. Fail loudly if the restructure accidentally breaks any logic

---

## Key insight: the oracle already exists

`torrust_mudlark::invariants::check_all_invariants()` checks four independent invariant
families (G-tree, V-tree, depth-gates, plateau). Calling it after every mutating operation
gives a free property-based oracle — no expected-value assertions needed for structural
correctness.

---

## Test file location

```
tests/
  integration.rs      ← one file, grows with the codebase
```

Rust integration tests in `tests/` can only use `pub` items — exactly what we want.

---

## Concrete test type — used in all tests

```rust
// u32 coordinate, u64 accumulator, N=16 (address space [0, 65535])
type TestGraph = torrust_mudlark::GvGraph<u32, u64, 16>;
```

These types satisfy all trait bounds (`Coordinate`, `Accumulator`, `Inspectable`) without
any custom implementation.

---

## Minimal Config helper

```rust
fn minimal_config() -> torrust_mudlark::Config<u64> {
    torrust_mudlark::Config {
        split_threshold: 10,
        depth_create: 2,
        depth_evict: 5,
        budget: None,
        alpha_relax: 0.5,
        bounded_eviction: false,
    }
}
```

---

## Tests to write (ordered by priority)

### 1. `new_graph_satisfies_invariants`
**Why:** Verifies the initial state (single root GNode, empty VTree) is structurally valid.  
**How:**
```rust
let graph = TestGraph::new(minimal_config());
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
```

---

### 2. `single_observe_satisfies_invariants`
**Why:** The very first `observe()` call allocates the first VNode. Exercises bootstrap path.  
**How:**
```rust
let mut graph = TestGraph::new(minimal_config());
graph.observe(1000_u32, 5_u64);
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
```

---

### 3. `repeated_observe_same_coord_satisfies_invariants`
**Why:** Repeated hits on the same coordinate should accumulate weight and eventually
trigger a split. Exercises `split`, `rebalance`, and `graph_budget`.  
**How:**
```rust
let mut graph = TestGraph::new(minimal_config());
for _ in 0..50 {
    graph.observe(1000_u32, 1_u64);
}
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
```

---

### 4. `observe_many_coords_satisfies_invariants`
**Why:** Observations spread across the coordinate space exercise the full G-tree routing,
multiple splits, and plateau tracking.  
**How:**
```rust
let mut graph = TestGraph::new(minimal_config());
for i in 0_u32..200 {
    graph.observe(i * 300, 1_u64);  // spread across [0, 65535]
}
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
```

---

### 5. `get_returns_zero_for_unobserved_coord`
**Why:** `get()` on a fresh graph should return a cell with zero accumulated value.  
**How:**
```rust
let graph = TestGraph::new(minimal_config());
let cell = graph.get(1000_u32);
assert_eq!(cell.value, 0_u64);
```

---

### 6. `get_returns_accumulated_value_after_observe`
**Why:** Verifies the read path reflects what was written.  
**How:**
```rust
let mut graph = TestGraph::new(minimal_config());
graph.observe(1000_u32, 42_u64);
let cell = graph.get(1000_u32);
assert!(cell.value > 0, "expected non-zero value after observe");
```

---

### 7. `observe_then_decay_satisfies_invariants`
**Why:** `decay()` is the only other mutating operation. Exercises temporal attenuation
and verifies invariants hold after weight reduction.  
**Requires:** `V: Attenuatable` — use `f64` accumulator for this test.

```rust
type DecayGraph = torrust_mudlark::GvGraph<u32, f64, 16>;

let config = torrust_mudlark::Config {
    split_threshold: 1.0,
    depth_create: 2,
    depth_evict: 5,
    budget: None,
    alpha_relax: 0.5,
    bounded_eviction: false,
};
let mut graph = DecayGraph::new(config);
for i in 0_u32..20 {
    graph.observe(i * 3000, 2.0_f64);
}
let root = graph.g_root();
graph.decay(root, 0.9, 0.01);
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
```

---

### 8. `bounded_budget_does_not_exceed_limit`
**Why:** When `budget` is set, eviction should keep the node count bounded. If this breaks
during restructuring, the graph will grow unboundedly.  
**How:**
```rust
let config = torrust_mudlark::Config {
    split_threshold: 1,
    depth_create: 2,
    depth_evict: 4,
    budget: Some(50),
    alpha_relax: 0.5,
    bounded_eviction: true,
};
let mut graph = TestGraph::new(config);
for i in 0_u32..500 {
    graph.observe((i * 127) % 65535, 1_u64);
}
let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
assert!(violations.is_empty(), "{violations:?}");
// node_count is accessible via a public method if exposed, or just check invariants
```

---

## Coverage summary

| Operation | Tests |
|-----------|-------|
| `GvGraph::new` | 1, 5 |
| `observe` (single) | 2, 6 |
| `observe` (repeated, same coord → splits) | 3 |
| `observe` (many coords → full tree) | 4 |
| `get` | 5, 6 |
| `decay` | 7 |
| bounded eviction | 8 |
| `check_all_invariants` oracle | 1, 2, 3, 4, 7, 8 |

---

## Definition of "done" (gate to start refactoring)

- [ ] All 8 tests compile and pass on the current code (`cargo test`)
- [ ] `cargo test` output shows 0 failures
- [ ] The tests are in `tests/integration.rs`, using only public API

Once this gate is green, the restructure can proceed because:
- Any broken `use crate::` path will produce a **compile error** (immediate feedback)
- Any accidentally broken logic will produce a **test failure** (invariant violation)
- The test file itself requires **zero changes** during the restructure
