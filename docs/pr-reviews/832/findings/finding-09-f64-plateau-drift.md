# Finding #9 — f64 Plateau Sum Drift After G-Node Split

**Status:** ❗ CONFIRMED OPEN  
**Severity:** Crash (debug builds) · Silent wrong data (release builds)  
**File:** `packages/mudlark/src/graph_plateau.rs`, line ~1387  
**Verified:** 2026-03-24 — still fires on rebased code (`da2ce7/20260305_mudlark` @ `5227ea5`)

---

## Description

`graph_plateau.rs` maintains each plateau's accumulated `f64` sum incrementally as observations arrive. After a G-node split, the incremental sum is verified against a fresh recompute by folding over the current basis elements using `assert_eq!` (exact equality).

Because `f64` addition is **non-associative**, the two accumulation paths traverse basis elements in different orders and produce sums that diverge by approximately 1 ULP (~10⁻¹⁵). The `assert_eq!` fires unconditionally in debug builds once this divergence occurs.

---

## Reproduction

```bash
cargo test -p torrust-mudlark --test reference_comparator bug_f64_plateau_drift_after_split -- --include-ignored
```

### Panic output

```
thread 'bug_f64_plateau_drift_after_split' panicked at
'assertion `left == right` failed: POST-OBSERVE: plateau sum drift at key BasisEdge(0)'
  delta=7.8140842050275685e-15
  left:  19.83806941676536
  right: 19.838069416765364
```

Delta ~7.8e-15, about 1 ULP. Left is the incrementally-accumulated value; right is the fresh recompute.

---

## Why the Author's Tests Do Not Catch It

The author's tests either:

- use too few observations to trigger the diverging split, or
- use integer types (`u64`) where `+` is exactly associative.

The drift only surfaces after enough `f64` observations that a G-node split changes the accumulation order mid-stream.

---

## Cameron's Related Fix (ADR-M-038) is a Different Bug

Finding ADR-M-038: the decay factor table was sized assuming tree depth ≤ N (the const generic), which does not hold for `f64` coordinates. Fixed in `decay.rs`.

**This is unrelated.** Our `assert_eq!` is at `graph_plateau.rs:1387`, not `decay.rs`. The ADR-M-038 fix does not resolve Finding #9. Verified 2026-03-24.

---

## Fix Options

1. **Approximate equality check** — Replace `assert_eq!(a, b, ...)` with an epsilon / ULP check. The `approx` crate provides `assert_ulps_eq!(a, b, max_ulps = 8)`. Smallest-scope fix.

2. **Remove the incremental path** — Accumulate plateau sums only by fresh recompute after each operation. Eliminates the invariant entirely at the cost of O(basis_size) per update.

3. **Guarantee bit-identical order** — Ensure both the incremental and recompute paths iterate basis elements in exactly the same order at all times. Fragile — any future refactor could silently reintroduce divergence.

Option 1 is the recommended minimal fix; option 2 is the most robust.

---

## Action Required

Report to Cameron on PR #832 with this detail. The key point: ADR-M-038 fixed a **different** panic in `decay.rs`; this panic in `graph_plateau.rs` is unresolved.
