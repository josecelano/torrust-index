# Model-Based / Property-Based Testing Strategy

> This document describes the approach used to stress-test `torrust-mudlark`
> with randomised operation sequences via `proptest`. It complements the
> fixed-seed reference comparator (`tests/reference_comparator.rs`) and the
> structural invariant checker (`src/invariants.rs`).

---

## The Oracle Problem

The central challenge in testing a complex data structure is identifying what
a "correct" implementation produces. Two approaches exist:

| Approach              | How oracle is derived                                          | When to use                                         |
| --------------------- | -------------------------------------------------------------- | --------------------------------------------------- |
| **Reference model**   | Simpler, obviously-correct implementation of the same contract | When a trivially-correct reference can be written   |
| **Property checking** | Algebraic laws the output must satisfy regardless of input     | When no simpler reference exists but laws are clear |

For `torrust-mudlark`, the oracle situation is:

- **Easy properties** (reference model available): `total_sum`, `range_sum`,
  plateau coverage, energy conservation under decay.
- **Hard properties** (no feasible reference): exact plateau boundary placement,
  V-Tree shape optimality, sample distribution accuracy without millions of draws.

This strategy focuses exclusively on the easy tier — properties verifiable
with a trivially-correct reference.

---

## Why proptest over fixed-seed tests?

`tests/reference_comparator.rs` already validates the easy properties, but uses
**fixed observation sequences** with deterministic seeds. This means:

- The same code paths are exercised on every run.
- Corner cases that require a specific _interleaving_ of operations are not explored.
- A bug that only manifests after a specific observe → decay → evict → observe
  sequence may never be found.

`proptest` generates the sequence itself, then **shrinks** any counterexample to
its minimal reproducing form. This is strictly more powerful for catching
interaction bugs.

---

## The Reference Model

```rust
/// Exact per-coordinate accumulator. O(N) space, O(1) observe, O(N) total_sum.
/// Obviously correct by inspection — no tree structure, no approximation.
struct VecAccumulator {
    totals: Vec<u64>,
}

impl VecAccumulator {
    fn observe(&mut self, coord: u64, delta: u64) {
        self.totals[coord as usize] = self.totals[coord as usize].saturating_add(delta);
    }
    fn total_sum(&self) -> u64 { self.totals.iter().sum() }
    fn range_sum(&self, lo: u64, hi: u64) -> u64 {
        self.totals[lo as usize..hi as usize].iter().sum()
    }
    fn apply_decay(&mut self, att: f64) {
        for v in &mut self.totals {
            // Mirror the mudlark u64 decay: floor(v * att)
            *v = (*v as f64 * att) as u64;
        }
    }
}
```

This reference is believable as a ground truth because:

1. **Complexity gap** — 10 lines vs ~4,000 lines of implementation. The
   probability of sharing an algorithmic bug is near zero.
2. **Directly implements the spec** — the spec says `total_sum` equals the sum
   of all deltas. This reference _is_ that sentence.
3. **No approximation** — no tree structure, no prorating, no rounding.

---

## Operation Model

Random operation sequences are drawn from this enum:

```rust
#[derive(Debug, Clone)]
enum Op {
    Observe { coord: u64, delta: u64 },
    DecayU64 { att_num: u32, att_den: u32 }, // att = att_num / att_den ∈ [0, 1]
    CheckEvictions,
}
```

Constraints on generated sequences:

- Coordinates are in `[0, 2^N)` for the chosen `N`.
- `delta` is in `[1, 1000]` to avoid trivial all-zero cases.
- Decay is represented as a rational `att_num / att_den` with `att_num ≤ att_den`
  to keep values in `[0.0, 1.0]`.
- Sequences of length `[1, 100]` — long enough to trigger eviction and splits.

---

## Properties Checked

### P1 — `total_sum` conservation

After any sequence of observe operations (no decay):

```
g.total_sum() == naive.total_sum()
```

**Oracle:** `VecAccumulator::total_sum()`.  
**Failure type:** exact integer mismatch.

### P2 — `range_sum` near-additivity

For any mid-point `m` in `[lo, hi]`:

```
|g.range_sum(lo..m) + g.range_sum(m..hi) − g.range_sum(lo..hi)| ≤ N
```

where `N = 8` (the G-tree depth constant).

**Oracle:** none needed — this is an internal consistency check.  
**Failure type:** discrepancy greater than `N`.

> Note: exact equality `== 0` was the first property tested. proptest found
> a counterexample immediately; see "Findings During Development" below.
> The tolerance of `N` reflects the maximum number of G-nodes on any descent
> path that can contribute independent pro-rating rounding errors.

### P3 — `range_sum` ≤ `total_sum`

```
g.range_sum(lo, hi) <= g.total_sum()
```

**Oracle:** none needed — monotonicity law.

### P4 — Structural invariants hold after every operation

After every observe/decay/evict in the sequence, `assert_invariants(&g)` must
not panic.

**Oracle:** `invariants::assert_invariants` (the implementation's own checker).  
**Failure type:** invariant violation panic.

### P5 — Plateau coverage

Plateaus returned by `g.plateaus()` must:

- Cover `[0, 2^N)` with no gaps.
- Be non-overlapping.
- Have their sums sum to ≈ `g.total_sum()`.

**Oracle:** iterate the returned iterator and check ranges and sums directly.

### P6 — After full decay (att=0), `total_sum` == 0

```rust
g.decay(root, 0.0, 0.0);
g.check_evictions();
assert_eq!(g.total_sum(), 0);
```

**Oracle:** trivial — annihilation must zero the index.

---

## What Properties Are NOT Checked Here

| Property                          | Why excluded                                                                                    |
| --------------------------------- | ----------------------------------------------------------------------------------------------- |
| Exact plateau boundary placement  | Requires reimplementing split logic — oracle as complex as implementation                       |
| `sample()` distribution accuracy  | Needs millions of draws; too slow for a property test                                           |
| V-Tree shape optimality (φ-bound) | Would require reimplementing the V-Tree — no simpler oracle                                     |
| f64 plateau sums                  | Finding #9: `assert_eq!` in `graph_plateau.rs` panics on f64 after splits; excluded pending fix |

---

## Results

See [`proptest-results/run-2026-03-25.txt`](proptest-results/run-2026-03-25.txt)
for the full run output and regression seeds.

---

## Findings During Development

### F-P2 — `range_sum` is not exactly additive for integer accumulators

**Discovered by:** proptest shrinking P2 to its minimal counterexample.

**Minimal case:**

```
ops = [Observe{coord:0, delta:3}, Observe{coord:0, delta:60},
       Observe{coord:0, delta:1}]
lo=0, m=1, hi=5
range_sum(0..1)=0, range_sum(1..5)=0, range_sum(0..5)=1
```

**Root cause:** `range_sum_inner` (ADR-M-020) pro-rates `g.own` by
`floor(own × overlap/width)` at nodes where the query boundary bisects the
node's spatial range. Because `floor(a) + floor(b) ≤ floor(a+b)`, the two
sub-queries can together undercount by up to 1 per overlapping node.

**Design status:** expected behaviour — the docstring already says "pro-rated
partial overlaps". The function is an _approximation_, not an exact aggregation.
This matters for users who assume range_sum is exactly additive.

**Action:** P2 weakened to tolerance `≤ N` (tree depth constant); no code
change to the implementation.

### F-P7 — `budget_config(32)` panics on construction

**Discovered by:** proptest calling `GvGraph::new` with an invalid config.

**Root cause:** with `depth_create=3`, `depth_evict=6`, `buffer=3`:
`budget_must_exceed = max(3^4, 2×2) = 81`. Budget of 32 is below this floor
(ADR-M-018).

**Action:** raised test constant from 32 to 100.

---

## Running the harness

```bash
# From the workspace root
CARGO_PROFILE_DEV_OPT_LEVEL=3 cargo test -p torrust-mudlark --test proptest_harness 2>&1 | tee /tmp/proptest-run.txt
```

To increase the number of test cases (default 256):

```bash
PROPTEST_CASES=1000 cargo test -p torrust-mudlark --test proptest_harness
```
