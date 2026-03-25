# Verification Strategy for PR #832

## The problem with traditional review

PR #832 adds ~64,000 lines of AI-assisted code. A traditional line-by-line review
is not feasible:

- The implementation is too large to read completely in a reasonable time.
- The formal proofs (φ-bounded eviction, V-Tree invariants V-I1–I9, plateau
  invariants P-I1–P-I4) require deep mathematical expertise to verify.
- The author likely did not read every line either — AI-generated code often
  contains subtly incorrect implementations of correct specifications.

**What we can say after Phases 1–4:** the design intent is sound, the public API
architecture is well-structured, and several concrete issues have been found
(MSRV bump, proxy regression, API doc gaps). What we cannot say is that the
implementation correctly realises the specification in every detail.

## The alternative: black-box verification

Instead of reading the implementation, we verify the _contract_ — the observable
behaviour promised by the public API. This is the same principle used in
formal testing: if you cannot prove correctness, falsify incorrectness.

The contract of `torrust-mudlark` is:

1. `observe(coord, intensity)` updates the index in O(log N).
2. `range_sum(start, end)` returns the total intensity in that range in O(log N).
3. `sample(&mut rng)` returns a coordinate sampled proportionally to intensity
   in O(log N).
4. The node count stays within the configured `node_budget` under sustained load.
5. Hot regions accumulate higher intensity than cold ones.

All five claims are testable without reading a single line of the implementation.

---

## Step 1 — Run the author's own tests

```bash
CARGO_PROFILE_DEV_OPT_LEVEL=3 cargo test -p torrust-mudlark
cargo test -p torrust-mudlark --release
cargo clippy -p torrust-mudlark --all-features -- -D warnings
cargo doc -p torrust-mudlark --no-deps
```

**What this tells us:** if the author's 833 tests fail, the PR is broken by its
own standard and cannot be merged. If they pass, we have a baseline to build on.

**Status:** ✅ `cargo test -p torrust-mudlark` exits 0 (observed 2026-03-16).

---

## Step 2 — Reference implementation comparator

**Status:** ✅ 17 passed, 0 failed, 1 ignored (2026-03-17)

A naive `VecAccumulator<u64>` that stores exact per-coordinate totals was
written as an independent ground truth. Tests cover 8 invariants:

| #   | Invariant                                                                      | Type  | Status |
| --- | ------------------------------------------------------------------------------ | ----- | ------ |
| 1   | `total_sum()` == naive total                                                   | exact | ✅     |
| 2   | `range_sum(..)` == `total_sum()`                                               | exact | ✅     |
| 3   | `range_sum(a..a)` == 0                                                         | exact | ✅     |
| 4   | `range_sum(A)` ≤ `total_sum()`                                                 | exact | ✅     |
| 5   | Monotone: positive obs never decrease range_sum                                | exact | ✅     |
| 6   | Binary partition with f64: left + right ≈ total                                | float | ✅     |
| 7   | Additive range split: `range_sum(a..m) + range_sum(m..b)` == `range_sum(a..b)` | float | ✅     |
| 8   | Single-coord cluster: total_sum matches expected                               | exact | ✅     |

File: `packages/mudlark/tests/reference_comparator.rs`

**New bug found (Finding #9 in REVIEW_PR_832.md):** Writing f64 invariant tests
revealed that `plateau_after_observe` in `graph_plateau.rs` uses `assert_eq!`
(exact equality) to compare two f64 sums accumulated via different orders. After
any G-node split, floating-point non-associativity causes ~1 ULP divergence and
the assertion always fires in `debug_assertions` builds. A `#[ignore]`d
regression test (`bug_f64_plateau_drift_after_split`) documents it. The f64
invariant tests use a high-split-threshold config to work around the bug.

```rust
// Naive: Vec<(u64, f64)> linear scan
struct NaiveIndex { events: Vec<(u64, f64)> }

impl NaiveIndex {
    fn observe(&mut self, coord: u64, intensity: f64) {
        self.events.push((coord, intensity));
    }
    fn range_sum(&self, start: u64, end: u64) -> f64 {
        self.events.iter()
            .filter(|(c, _)| *c >= start && *c < end)
            .map(|(_, i)| i)
            .sum()
    }
}

// Then for N random observations:
let naive_sum = naive.range_sum(a, b);
let mudlark_sum = graph.range_sum(a, b);
assert!((naive_sum - mudlark_sum).abs() / naive_sum.max(1.0) < 1e-6);
```

This does not require understanding any internal detail — just the contract.
Any divergence is a real bug with a concrete reproducer.

**Caveat:** decay means the two structures will diverge over time if decay is
applied. The comparator should either disable decay or apply the same decay
function to the naive accumulators.

---

## Step 3 — Use-case acceptance tests

Each problem in [USE_CASES.md](USE_CASES.md) has a clear expected outcome. Write
one test per use case that verifies the outcome holds.

### Use case 1 — IP-prefix flood detection

```rust
// Simulate 10,000 announces from 1.2.3.x and 100 from random IPs.
// After observations, the /24 around 1.2.3.0 should dominate.
let flood_sum = graph.range_sum(encode_ip(1,2,3,0), encode_ip(1,2,4,0));
let total_sum = graph.range_sum(0, u32::MAX as u64 + 1);
assert!(flood_sum / total_sum > 0.90);
// sample() should land in the flood range most of the time
let samples: Vec<_> = (0..1000).map(|_| graph.sample(&mut rng)).collect();
let in_range = samples.iter().filter(|&&c| c >= encode_ip(1,2,3,0)
    && c < encode_ip(1,2,4,0)).count();
assert!(in_range > 800);
```

### Use case 4 — Infohash neighbourhood flood

```rust
// Simulate 50,000 announces near infohash prefix 0xDEADBEEF
// and 500 uniformly distributed.
// The neighbourhood [0xDEADBEEF_00.., 0xDEADBEEF_FF..] should dominate.
let flood_sum = graph.range_sum(0xDEADBEEF_00000000, 0xDEADFF0000000000);
let total_sum = graph.range_sum(0, u64::MAX);
assert!(flood_sum / total_sum > 0.90);
```

### Memory budget invariant (all use cases)

```rust
let config = Config { node_budget: 1000, .. };
let mut graph = GvGraph::new(config);
for _ in 0..1_000_000 {
    graph.observe(rng.next_u64(), 1.0);
    graph.decay(0.999, 0);
}
assert!(graph.node_count() <= 1000,
    "node count {} exceeded budget 1000", graph.node_count());
```

This is the single most important invariant. If it fails, the memory-safety
claim of the entire PR is wrong.

---

## Step 4 — Mutation testing (optional, tooling-assisted)

```bash
cargo install cargo-mutants
cargo mutants -p torrust-mudlark --timeout 60
```

`cargo-mutants` randomly modifies the implementation (flips operators, removes
conditions, etc.) and checks whether the existing tests catch the change. A
**kill rate** above ~80% means the tests are meaningful and dense. A low kill
rate means the tests are superficial — which is itself a finding worth reporting
to the author.

This step requires no reading of the implementation at all. Let the tooling do it.

---

## Step 5 — Static analysis

```bash
# Memory safety (no unsafe anyway, but good hygiene)
cargo audit -p torrust-mudlark

# Unused dependencies
cargo machete

# License compatibility
cargo deny check licenses
```

---

## What to write in the review

After completing the steps above, the review can honestly say:

> **Correctness is verified by:**
>
> 1. The author's 833 tests passing.
> 2. Independent use-case tests verifying the public contract.
> 3. The memory budget invariant holding under 1,000,000 observations.
>
> **Correctness is NOT verified by:**
>
> - Line-by-line inspection of the implementation.
> - Independent proof of the φ-bounded eviction theorems.
> - Full mutation testing (though the kill rate gives a signal).
>
> **Known issues (from Phases 1–4) must be addressed before merge regardless
> of the outcome of these tests.**

This is an honest, defensible position for a 64k-line AI-assisted PR.

---

## Progress

| Step | Description                                | Status                                                             |
| ---- | ------------------------------------------ | ------------------------------------------------------------------ |
| 1    | Run author's tests                         | ✅ Passes (1,311 tests)                                            |
| 2    | Reference implementation comparator        | ✅ 17 passed, 1 bug found (Finding #9)                             |
| 3    | Use-case acceptance tests                  | ⬜ Not written                                                     |
| 4    | Mutation testing                           | ✅ 60.1% kill rate (413 missed)                                    |
| 5    | Static analysis                            | ⬜ Not run                                                         |
| 6    | Property-based / model-based testing (new) | See [PROPTEST_STRATEGY.md](PROPTEST_STRATEGY.md) and results below |

---

## Step 6 — Property-Based Testing (proptest)

Fixed-seed tests in `reference_comparator.rs` validate the contract but explore
the same code paths on every run. Property-based testing generates random
operation sequences and checks algebraic laws after each step — covering
interaction bugs that fixed sequences miss.

**Full strategy document:** [PROPTEST_STRATEGY.md](PROPTEST_STRATEGY.md)

**Results:** [proptest-results/proptest-2026-03-25.md](proptest-results/proptest-2026-03-25.md)

Properties checked:

| #   | Property                                                         | Type  |
| --- | ---------------------------------------------------------------- | ----- |
| P1  | `total_sum` == naive after any observe sequence                  | exact |
| P2  | `range_sum` additivity: `sum(lo..m) + sum(m..hi) == sum(lo..hi)` | exact |
| P3  | `range_sum` ≤ `total_sum` (monotonicity)                         | exact |
| P4  | Structural invariants hold after every operation                 | exact |
| P5  | Plateaus cover `[0, 2^N)` with no gaps                           | exact |
| P6  | Full decay (att=0) → `total_sum == 0`                            | exact |
