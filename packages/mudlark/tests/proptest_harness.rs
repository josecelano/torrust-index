// SPDX-License-Identifier: AGPL-3.0-only
// SPDX-FileCopyrightText: 2026 Torrust project contributors

//! Property-based tests for `torrust-mudlark` using `proptest`.
//!
//! Checks algebraic laws that must hold for any sequence of operations,
//! using a trivially-correct `VecAccumulator` as the ground-truth oracle
//! where needed.
//!
//! See `docs/pr-reviews/832/PROPTEST_STRATEGY.md` for the full rationale.
//!
//! # Test index
//!
//! | Test | Property | Oracle |
//! |------|----------|--------|
//! | [`p1_total_sum_conservation`] | `total_sum` == naive after any observe sequence | `VecAccumulator` |
//! | [`p2_range_sum_additivity`] | `|range_sum(lo..m) + range_sum(m..hi) − range_sum(lo..hi)| ≤ N` (pro-rating tolerance) | none (consistency) |
//! | [`p3_range_sum_le_total_sum`] | `range_sum` ≤ `total_sum` | none (monotonicity law) |
//! | [`p4_invariants_hold_after_every_op`] | structural invariants hold after every mutation | `assert_invariants` |
//! | [`p5_plateaus_cover_domain`] | plateaus tile `[0, 2^N)` with no gaps or overlaps | none (structural) |
//! | [`p6_annihilation_zeroes_total`] | `decay(root, 0, 0)` + evict → `total_sum == 0` | trivial |
//! | [`p7_budget_never_exceeded`] | `node_count` ≤ budget after any sequence | trivial |

use proptest::prelude::*;
use torrust_mudlark::GvGraph;
use torrust_mudlark::invariants::assert_invariants;
use torrust_mudlark::testing::{budget_config, default_config};

// ── Domain constant ──────────────────────────────────────────────────────────

/// All tests use N=8 (domain [0, 256)).  Small enough to be fast,
/// large enough to trigger splits and evictions.
const N: u32 = 8;
const DOMAIN: u64 = 1u64 << N; // 256

// ── Naive reference accumulator ──────────────────────────────────────────────

/// Exact per-coordinate accumulator.  O(DOMAIN) space, O(1) observe,
/// O(DOMAIN) total_sum.  Trivially correct by inspection.
struct VecAccumulator {
    totals: Vec<u64>,
}

impl VecAccumulator {
    fn new() -> Self {
        Self {
            totals: vec![0u64; DOMAIN as usize],
        }
    }

    fn observe(&mut self, coord: u64, delta: u64) {
        let i = coord as usize;
        self.totals[i] = self.totals[i].saturating_add(delta);
    }

    fn total_sum(&self) -> u64 {
        self.totals.iter().sum()
    }
}

// ── Operation model ───────────────────────────────────────────────────────────

/// A single operation that can be applied to a `GvGraph<u64, u64, N>`.
#[derive(Debug, Clone)]
enum Op {
    Observe { coord: u64, delta: u64 },
    CheckEvictions,
}

/// proptest strategy for a single Op.
fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        // ~85% observations
        4 => (0u64..DOMAIN, 1u64..=1000u64).prop_map(|(coord, delta)| Op::Observe { coord, delta }),
        // ~15% eviction checks
        1 => Just(Op::CheckEvictions),
    ]
}

/// proptest strategy for a sequence of 1–100 Ops.
fn arb_ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(arb_op(), 1..=100)
}

// ── P1: total_sum conservation ───────────────────────────────────────────────

proptest! {
    /// P1 — `total_sum()` must equal the naive sum of all deltas applied,
    /// for any sequence of observe operations.
    #[test]
    fn p1_total_sum_conservation(ops in arb_ops()) {
        let mut g = GvGraph::<u64, u64, N>::new(default_config());
        let mut naive = VecAccumulator::new();

        for op in &ops {
            match op {
                Op::Observe { coord, delta } => {
                    g.observe(*coord, *delta);
                    naive.observe(*coord, *delta);
                }
                Op::CheckEvictions => { let _ = g.check_evictions(); }
            }
        }

        prop_assert_eq!(
            g.total_sum(),
            naive.total_sum(),
            "total_sum mismatch after {} ops",
            ops.len()
        );
    }
}

// ── P2: range_sum additivity ─────────────────────────────────────────────────

proptest! {
    /// P2 — For any split point `m` in `(lo, hi)`, the sum of the two
    /// half-range queries must agree with the full-range query to within
    /// ±N (the descent depth of the G-tree).
    ///
    /// Exact equality would require all query/split boundaries to align
    /// with G-node edges.  In general the pro-rated `f64 × own` values
    /// for partial overlaps are truncated independently per sub-query,
    /// introducing up to N ULPs of error (the number of partially-overlapping
    /// nodes on the descent path).
    #[test]
    fn p2_range_sum_additivity(
        ops in arb_ops(),
        lo in 0u64..DOMAIN,
        span in 2u64..=DOMAIN,
        mid_frac in 1u64..=99u64,
    ) {
        let hi = (lo + span).min(DOMAIN);
        if hi <= lo + 1 { return Ok(()); }
        let m = lo + 1 + (mid_frac * (hi - lo - 1) / 100);
        let m = m.clamp(lo + 1, hi - 1);

        let mut g = GvGraph::<u64, u64, N>::new(default_config());
        for op in &ops {
            if let Op::Observe { coord, delta } = op {
                g.observe(*coord, *delta);
            }
        }

        let left = g.range_sum(lo..m);
        let right = g.range_sum(m..hi);
        let total = g.range_sum(lo..hi);

        // range_sum uses f64 pro-rating for nodes that only partially overlap
        // the query boundary.  The pro-rating applies independently to each
        // sub-query and to the combined query, so integer truncation can
        // introduce a discrepancy bounded by the number of G-nodes on the
        // descent path (O(N = 8)).  A difference of 0 is expected when both
        // boundaries align exactly with G-node edges; otherwise ≤ N is the
        // tolerance (ADR-M-020, range_sum docstring: "pro-rated partial
        // overlaps").
        let combined = left.saturating_add(right);
        let diff = if combined >= total { combined - total } else { total - combined };
        prop_assert!(
            diff <= N as u64,
            "range_sum({}..{}) + range_sum({}..{}) differs from range_sum({}..{}) by {} (> N={}): {} + {} vs {}",
            lo, m, m, hi, lo, hi, diff, N, left, right, total
        );
    }
}

// ── P3: range_sum ≤ total_sum ────────────────────────────────────────────────

proptest! {
    /// P3 — `range_sum(lo..hi) ≤ total_sum()` for any valid range.
    #[test]
    fn p3_range_sum_le_total_sum(
        ops in arb_ops(),
        lo in 0u64..DOMAIN,
        span in 0u64..=DOMAIN,
    ) {
        let hi = (lo + span).min(DOMAIN);

        let mut g = GvGraph::<u64, u64, N>::new(default_config());
        for op in &ops {
            if let Op::Observe { coord, delta } = op {
                g.observe(*coord, *delta);
            }
        }

        prop_assert!(
            g.range_sum(lo..hi) <= g.total_sum(),
            "range_sum({lo}..{hi}) > total_sum()"
        );
    }
}

// ── P4: structural invariants after every op ─────────────────────────────────

proptest! {
    /// P4 — Structural invariants must hold after each operation in any
    /// sequence of observe + check_evictions calls.
    #[test]
    fn p4_invariants_hold_after_every_op(ops in arb_ops()) {
        let mut g = GvGraph::<u64, u64, N>::new(default_config());

        for (i, op) in ops.iter().enumerate() {
            match op {
                Op::Observe { coord, delta } => g.observe(*coord, *delta),
                Op::CheckEvictions => { let _ = g.check_evictions(); }
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert_invariants(&g);
            }));
            prop_assert!(
                result.is_ok(),
                "Invariant violated after op {i} ({op:?})"
            );
        }
    }
}

// ── P5: plateaus cover the domain ────────────────────────────────────────────

proptest! {
    /// P5 — Plateaus returned by `g.plateaus()` must cover `[0, 2^N)`
    /// with no gaps and no overlaps.
    #[test]
    fn p5_plateaus_cover_domain(ops in arb_ops()) {
        let mut g = GvGraph::<u64, u64, N>::new(default_config());
        for op in &ops {
            if let Op::Observe { coord, delta } = op {
                g.observe(*coord, *delta);
            }
        }

        let plateau_map = g.plateaus();

        // Collect plateau ranges in key order (BasisEdge key order == start coord order).
        let ranges: Vec<(u64, u64)> = plateau_map
            .values()
            .map(|p| (p.start, p.end))
            .collect();

        prop_assume!(!ranges.is_empty());

        let mut cursor = 0u64;
        for (start, end) in &ranges {
            prop_assert_eq!(
                *start, cursor,
                "gap or overlap: expected start={}, got start={}",
                cursor, start
            );
            prop_assert!(*end > *start, "zero-width plateau at {}", start);
            cursor = *end;
        }
        prop_assert_eq!(cursor, DOMAIN, "plateaus do not reach end of domain: cursor={}", cursor);
    }
}

// ── P6: annihilation zeroes the tree ─────────────────────────────────────────

proptest! {
    /// P6 — After `decay(root, 0.0, 0.0)` + `check_evictions()`,
    /// `total_sum()` must be 0.
    #[test]
    fn p6_annihilation_zeroes_total(ops in arb_ops()) {
        let mut g = GvGraph::<u64, u64, N>::new(default_config());
        for op in &ops {
            if let Op::Observe { coord, delta } = op {
                g.observe(*coord, *delta);
            }
        }

        let root = g.g_root();
        g.decay(root, 0.0, 0.0);
        let _ = g.check_evictions();

        prop_assert_eq!(
            g.total_sum(),
            0u64,
            "total_sum should be 0 after full annihilation"
        );
    }
}

// ── P7: budget is never exceeded ─────────────────────────────────────────────

proptest! {
    /// P7 — When a node budget is configured, `node_count()` must never
    /// exceed it after any sequence of operations.
    #[test]
    fn p7_budget_never_exceeded(ops in arb_ops()) {
        // With depth_create=3, depth_evict=6 (buffer=3):
        //   headroom = max(3^(3+1), 2*(3-1)) = max(81, 4) = 81
        //   budget must be > 81  (ADR-M-018).
        const BUDGET: usize = 100;
        let mut g = GvGraph::<u64, u64, N>::new(budget_config(BUDGET));

        for op in &ops {
            match op {
                Op::Observe { coord, delta } => g.observe(*coord, *delta),
                Op::CheckEvictions => { let _ = g.check_evictions(); }
            }
            prop_assert!(
                g.node_count() <= u32::try_from(BUDGET).unwrap(),
                "node_count {} exceeded budget {}",
                g.node_count(), BUDGET
            );
        }
    }
}
