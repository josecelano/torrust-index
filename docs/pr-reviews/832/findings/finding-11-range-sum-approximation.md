# Finding #11 — `range_sum` Approximation Not Clearly Documented

**Status:** ⚠️ Open — documentation gap  
**Severity:** Usability (API contract not clearly communicated)  
**File:** `packages/mudlark/src/graph_query.rs` + `docs/api.md`  
**Discovered:** 2026-03-25 via proptest (property P2 `range_sum` additivity)  
**Reported:** [PR #832 comment](https://github.com/torrust/torrust-index/pull/832#issuecomment-4126567872)

---

## Description

`range_sum` returns an **estimate**, not an exact value — even for integer
accumulators. Accumulated values are pro-rated using `f64` arithmetic at
G-nodes that are only partially covered by the query range (ADR-M-020). The
result converges toward the true sum as the G-tree refines depth, but for
integer accumulators, `range_sum(x..x+1)` will often return `0` until `x` has
received enough observations to trigger a split.

This is documented in the `range_sum` docstring ("pro-rated partial overlaps",
ADR-M-020), but not prominently enough — the approximation note is easy to
miss and the severity for integer accumulators is not called out.

---

## Proptest Counterexample

Property P2 (`range_sum(lo..m) + range_sum(m..hi) == range_sum(lo..hi)`) was
immediately falsified by proptest with this minimal counterexample:

```
observe(0, 64)
range_sum(0..1) = 0    // coord 0 holds 64 units, but returns 0
range_sum(1..5) = 0
range_sum(0..5) = 1
```

`range_sum(0..1)` returns `0` even though coord `0` holds 64 units of energy,
because the energy sits in the root node covering `[0, 256)` and is pro-rated
by `1/256`. Floor division of `64 / 256 = 0.25` truncates to `0`.

Full regression seed in `tests/proptest_harness.proptest-regressions`:

```
cc ac101aa0aa99475a29a60339591e6fed63fcaa7f2c31206a7d385d100fd1004e
  shrinks to: ops=[Observe{coord:0,delta:3},Observe{coord:0,delta:60},
              Observe{coord:0,delta:1}], lo=0, span=5, mid_frac=1
```

---

## Root Cause

`range_sum_inner` (ADR-M-020) pro-rates `g.own` by
`floor(own × overlap_width / node_width)` whenever the query boundary bisects
a G-node. Until the G-tree has built enough depth at the queried coordinates to
give each one its own fine leaf, all energy sits in coarse ancestor nodes and
is scaled down. For small windows or small accumulator values, this truncates
to zero.

The additivity property `range_sum(lo..m) + range_sum(m..hi) == range_sum(lo..hi)`
only holds exactly when all query boundaries coincide with existing G-node
edges.

---

## User Impact

A user with an integer accumulator will reasonably write:

```rust
g.observe(42, 100);
assert_eq!(g.range_sum(42..43), 100);  // FAILS — returns 0
```

and receive no hint from the docs or compiler that this isn't supposed to work.
The library is a density estimator, not an exact counter — but this expectation
gap isn't communicated at the level of visibility it deserves.

---

## Suggested Fix

Add an `# Approximation` section to the `range_sum` docstring:

> **Approximation note:** `range_sum` returns an estimate, not an exact count.
> Accumulated values are pro-rated using `f64` arithmetic at G-nodes only
> partially covered by the query range (ADR-M-020). The result converges toward
> the true sum as the G-tree refines depth at the queried coordinates, but for
> integer accumulators, `range_sum(x..x+1)` will often return `0` until `x` has
> received enough observations to trigger a split.

The current docstring uses `assert!(g.range_sum(0..128) >= 3)` (not `==`), which
hints at approximation but doesn't explain _why_ or what the implication is for
integer accumulators.

---

## Proptest Resolution

Property P2 was loosened to allow `|combined - total| <= N` (where N=8 for the
test domain depth) instead of exact equality. The P2 fix is in:
[proptest-results/run-2026-03-25.txt](../proptest-results/run-2026-03-25.txt)

All 7 proptest properties now pass. See
[PROPTEST_STRATEGY.md](../PROPTEST_STRATEGY.md) for the full strategy.
