// SPDX-License-Identifier: AGPL-3.0-only
// SPDX-FileCopyrightText: 2026 Torrust project contributors

//! Reference implementation comparator for `GvGraph`.
//!
//! A naive [`VecAccumulator`] that stores per-coordinate exact totals
//! serves as the ground truth for `total_sum`.  Because `range_sum` is
//! a *spatial approximation* (values in nodes are pro-rated across the
//! node's dyadic range), the comparisons in this file target the precise
//! invariants that **must** hold regardless of tree shape or config:
//!
//! | # | Invariant | Type |
//! |---|-----------|------|
//! | 1 | `total_sum()` == naive total | exact |
//! | 2 | `range_sum(..)` == `total_sum()` | exact |
//! | 3 | `range_sum(a..a)` == 0 | exact |
//! | 4 | `range_sum(A)` ≤ `total_sum()` | exact |
//! | 5 | Monotone: positive observations never decrease range_sum | exact |
//! | 6 | Binary partition with f64 V: `left + right ≈ total` | floating-point |
//! | 7 | Additive range split: `range_sum(a..m) + range_sum(m..b)` == `range_sum(a..b)` | floating-point |
//! | 8 | Single-coord cluster: total_sum matches expected after many observations | exact |
//!
//! The f64 invariants (6-7) hold to within `f64::EPSILON * total_sum * node_count`
//! because `scale_by` for f64 returns `self * ratio` directly (no truncation).

use torrust_mudlark::invariants::assert_invariants;
use torrust_mudlark::testing::{aggressive_config, default_config, f64_default_config, range_tree_config, TestLcgRng};
use torrust_mudlark::{Config, GvGraph};

// ── Naive reference accumulator ─────────────────────────────────────────────

/// Exact per-coordinate accumulator; no tree structure, no approximation.
struct VecAccumulator {
    totals: Vec<u64>,
    domain: usize,
}

impl VecAccumulator {
    fn new(domain: usize) -> Self {
        Self {
            totals: vec![0u64; domain],
            domain,
        }
    }

    fn observe(&mut self, coord: u64, delta: u64) {
        let c = coord as usize;
        assert!(c < self.domain, "coord {c} out of domain {}", self.domain);
        self.totals[c] = self.totals[c].saturating_add(delta);
    }

    fn range_sum(&self, lo: u64, hi: u64) -> u64 {
        self.totals[lo as usize..hi as usize].iter().sum()
    }

    fn total_sum(&self) -> u64 {
        self.totals.iter().sum()
    }
}

// ── Observation helpers ──────────────────────────────────────────────────────

/// Feed `count` deterministic pseudo-random observations to a `GvGraph<u64,
/// u64, N>` and a `VecAccumulator`.
///
/// Returns the exact sum of all deltas applied.
fn feed_both_u64<const N: u32>(g: &mut GvGraph<u64, u64, N>, naive: &mut VecAccumulator, seed: u64, count: usize) -> u64 {
    let domain = 1u64 << N;
    let mut rng = TestLcgRng(seed);
    let mut total = 0u64;
    for _ in 0..count {
        let coord = (rng.next_f64() * domain as f64) as u64;
        // Keep coordinate in [0, domain) strictly.
        let coord = coord.min(domain - 1);
        let delta = 1u64 + (rng.next_f64() * 9.0) as u64; // [1, 10]
        g.observe(coord, delta);
        naive.observe(coord, delta);
        total = total.saturating_add(delta);
    }
    total
}

/// Feed `count` observations to a `GvGraph<u64, f64, N>` and a separate f64
/// total tracker.  Returns the exact f64 sum of all deltas.
fn feed_f64<const N: u32>(g: &mut GvGraph<u64, f64, N>, seed: u64, count: usize) -> f64 {
    let domain = 1u64 << N;
    let mut rng = TestLcgRng(seed);
    let mut total = 0.0_f64;
    for _ in 0..count {
        let coord = (rng.next_f64() * domain as f64) as u64;
        let coord = coord.min(domain - 1);
        let delta = 1.0_f64 + (rng.next_f64() * 9.0); // [1.0, 10.0)
        g.observe(coord, delta);
        total += delta;
    }
    total
}

// ── Invariant 1: total_sum exactness ────────────────────────────────────────

/// `GvGraph::total_sum()` must exactly equal the naive sum of every delta
/// applied, for any config and any sequence of observations.
#[test]
fn total_sum_matches_naive_sum_default_config() {
    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    let expected = feed_both_u64(&mut g, &mut naive, 0xDEAD_BEEF, 2_000);
    assert_eq!(g.total_sum(), expected, "total_sum must equal sum of all deltas");
    assert_eq!(g.total_sum(), naive.total_sum(), "total_sum must match naive");
    assert_invariants(&g);
}

#[test]
fn total_sum_matches_naive_sum_aggressive_config() {
    let mut g = GvGraph::<u64, u64, 8>::new(aggressive_config());
    let mut naive = VecAccumulator::new(256);
    let expected = feed_both_u64(&mut g, &mut naive, 0xCAFE_F00D, 1_000);
    assert_eq!(g.total_sum(), expected);
    assert_eq!(g.total_sum(), naive.total_sum());
    assert_invariants(&g);
}

#[test]
fn total_sum_matches_naive_sum_range_tree_config() {
    let mut g = GvGraph::<u64, u64, 8>::new(range_tree_config());
    let mut naive = VecAccumulator::new(256);
    let expected = feed_both_u64(&mut g, &mut naive, 0x1234_5678, 500);
    assert_eq!(g.total_sum(), expected);
    assert_eq!(g.total_sum(), naive.total_sum());
    assert_invariants(&g);
}

#[test]
fn total_sum_zero_on_empty_graph() {
    let g = GvGraph::<u64, u64, 8>::new(default_config());
    assert_eq!(g.total_sum(), 0u64);
    assert_invariants(&g);
}

// ── Invariant 2: full-domain range_sum equals total_sum ─────────────────────

/// `range_sum(..)` and `range_sum(0..2^N)` must equal `total_sum()` exactly.
#[test]
fn full_domain_range_sum_equals_total_sum() {
    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    feed_both_u64(&mut g, &mut naive, 0xABCD_1234, 1_500);
    let total = g.total_sum();
    assert_eq!(g.range_sum(..), total, "range_sum(..) must equal total_sum");
    assert_eq!(g.range_sum(0u64..256u64), total, "range_sum(0..256) must equal total_sum");
    assert_invariants(&g);
}

#[test]
fn full_domain_range_sum_equals_total_sum_f64() {
    let mut g = GvGraph::<u64, f64, 8>::new(f64_default_config());
    let expected = feed_f64(&mut g, 0x9999_AAAA, 1_000);
    let total = g.total_sum();
    assert!(
        (total - expected).abs() < 1e-6,
        "f64 total_sum={total} should equal naive sum={expected}"
    );
    assert!(
        (g.range_sum(..) - total).abs() < 1e-9,
        "f64 range_sum(..) must equal total_sum"
    );
    assert_invariants(&g);
}

// ── Invariant 3: empty range ─────────────────────────────────────────────────

#[test]
fn empty_range_sum_is_zero() {
    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    feed_both_u64(&mut g, &mut naive, 0x1111, 300);
    // Zero-length ranges.
    assert_eq!(g.range_sum(0u64..0u64), 0, "empty range [0,0)");
    assert_eq!(g.range_sum(100u64..100u64), 0, "empty range [100,100)");
    assert_eq!(g.range_sum(255u64..255u64), 0, "empty range [255,255)");
    assert_invariants(&g);
}

// ── Invariant 4: range_sum never exceeds total_sum ───────────────────────────

#[test]
fn range_sum_never_exceeds_total_sum() {
    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    feed_both_u64(&mut g, &mut naive, 0x2222, 800);
    let total = g.total_sum();
    // Spot-check several ranges.
    for &(lo, hi) in &[
        (0u64, 1u64),
        (0, 64),
        (64, 128),
        (128, 256),
        (0, 128),
        (100, 200),
        (10, 250),
        (0, 255),
    ] {
        let s = g.range_sum(lo..hi);
        assert!(s <= total, "range_sum({lo}..{hi})={s} must not exceed total_sum={total}");
    }
    assert_invariants(&g);
}

// ── Invariant 5: monotonicity under positive additions ───────────────────────

/// Adding positive observations must never decrease `range_sum` for any range.
#[test]
fn range_sum_is_monotone_after_positive_observations() {
    let bins: Vec<(u64, u64)> = (0u64..8u64).map(|i| (i * 32, (i + 1) * 32)).collect();

    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    feed_both_u64(&mut g, &mut naive, 0x3333, 400);

    // Snapshot before additional observations.
    let before: Vec<u64> = bins.iter().map(|&(lo, hi)| g.range_sum(lo..hi)).collect();

    // Add more observations.
    feed_both_u64(&mut g, &mut naive, 0x4444, 400);

    // Each bin must be >= before.
    for (i, (&(lo, hi), &before_val)) in bins.iter().zip(before.iter()).enumerate() {
        let after = g.range_sum(lo..hi);
        assert!(
            after >= before_val,
            "bin {i} [{lo}..{hi}): range_sum decreased from {before_val} to {after} after adding obs"
        );
    }
    assert_invariants(&g);
}

// ── Invariant 6: binary partition with f64 accumulator ──────────────────────

/// For `f64` V, `range_sum(0..m) + range_sum(m..2^N)` should equal
/// `total_sum()` to floating-point precision (no integer truncation).
///
/// The error is bounded by the number of G-nodes times `f64::EPSILON`,
/// so we use a generous tolerance of 1e-6 * total.
#[test]
fn binary_partition_sum_equals_total_f64() {
    let mut g = GvGraph::<u64, f64, 8>::new(f64_default_config());
    feed_f64(&mut g, 0x5555, 1_000);

    let total = g.total_sum();
    let eps = (total * 1e-6).max(1e-9);

    let left = g.range_sum(0u64..128u64);
    let right = g.range_sum(128u64..256u64);
    assert!(
        (left + right - total).abs() < eps,
        "binary partition: left={left} + right={right} = {} ≠ total={total} (eps={eps})",
        left + right
    );
    assert_invariants(&g);
}

/// Quad partition with f64: four equal bins should sum to `total_sum`.
#[test]
fn quadrant_partition_sum_equals_total_f64() {
    let mut g = GvGraph::<u64, f64, 8>::new(f64_default_config());
    feed_f64(&mut g, 0x6666, 800);

    let total = g.total_sum();
    let eps = (total * 1e-6).max(1e-9);

    let q: f64 = [0u64, 64, 128, 192]
        .iter()
        .zip([64u64, 128, 192, 256].iter())
        .map(|(&lo, &hi)| g.range_sum(lo..hi))
        .sum();
    assert!((q - total).abs() < eps, "quad partition sum={q} ≠ total={total} (eps={eps})");
    assert_invariants(&g);
}

// ── Invariant 7: additive range split (f64) ──────────────────────────────────

/// `range_sum(a..b) == range_sum(a..m) + range_sum(m..b)` for any m ∈ (a, b),
/// holding to floating-point precision with f64 accumulator.
#[test]
fn additive_range_split_f64() {
    let mut g = GvGraph::<u64, f64, 8>::new(f64_default_config());
    feed_f64(&mut g, 0x7777, 600);

    let total = g.total_sum();
    let eps = (total * 1e-6).max(1e-9);

    // Various mid-points — both dyadic and non-dyadic.
    for &mid in &[1u64, 10, 50, 64, 100, 128, 150, 200, 250] {
        let a = 0u64;
        let b = 256u64;
        let full = g.range_sum(a..b);
        let l = g.range_sum(a..mid);
        let r = g.range_sum(mid..b);
        assert!(
            (l + r - full).abs() < eps,
            "additive split at mid={mid}: {l} + {r} = {} ≠ {full} (eps={eps})",
            l + r
        );
    }

    // Sub-range splits.
    for &(a, b, mid) in &[(0u64, 200u64, 100u64), (10, 150, 80), (64, 192, 128)] {
        let full = g.range_sum(a..b);
        let l = g.range_sum(a..mid);
        let r = g.range_sum(mid..b);
        assert!(
            (l + r - full).abs() < eps,
            "sub-range [{a},{mid},{b}): {l} + {r} ≠ {full} (eps={eps})"
        );
    }
    assert_invariants(&g);
}

// ── Invariant 8: single-coord cluster total ───────────────────────────────────

/// Observing the same coordinate many times must give an exact `total_sum`.
/// The tree restructures around the hot spot, but the total is invariant.
#[test]
fn single_coord_cluster_total_sum_is_exact() {
    let coord = 42u64;
    let delta = 3u64;
    let count = 500u64;

    let mut g = GvGraph::<u64, u64, 8>::new(aggressive_config()); // low split threshold
    for _ in 0..count {
        g.observe(coord, delta);
    }
    let expected = delta * count;
    assert_eq!(
        g.total_sum(),
        expected,
        "single-coord cluster: total_sum should be {expected}"
    );
    assert_invariants(&g);
}

/// Spreading observations across exactly two coordinates must give exact total.
#[test]
fn two_coord_total_sum_is_exact() {
    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let n = 200;
    for i in 0..n {
        g.observe(0u64, 1u64);
        g.observe(255u64, u64::from(i % 5 + 1));
    }
    // total = n * 1 + sum(i % 5 + 1 for i in 0..n)
    let expected_at_255: u64 = (0u64..n).map(|i| i % 5 + 1).sum();
    let expected = n + expected_at_255;
    assert_eq!(g.total_sum(), expected);
    assert_invariants(&g);
}

// ── Cross-config consistency ─────────────────────────────────────────────────

/// Given the same observations, `total_sum` must be identical regardless of
/// whether we use `default_config` or `aggressive_config`.
#[test]
fn total_sum_is_config_independent() {
    const SEED: u64 = 0x8888;
    const COUNT: usize = 600;

    let mut g_default = GvGraph::<u64, u64, 8>::new(default_config());
    let mut g_aggressive = GvGraph::<u64, u64, 8>::new(aggressive_config());
    let mut naive = VecAccumulator::new(256);

    // Same observations to all three.
    let mut naive2 = VecAccumulator::new(256);
    let domain = 256u64;
    let mut rng = TestLcgRng(SEED);
    for _ in 0..COUNT {
        let coord = (rng.next_f64() * domain as f64) as u64;
        let coord = coord.min(domain - 1);
        let delta = 1u64 + (rng.next_f64() * 9.0) as u64;
        g_default.observe(coord, delta);
        g_aggressive.observe(coord, delta);
        naive.observe(coord, delta);
        naive2.observe(coord, delta);
    }

    let total = naive.total_sum();
    assert_eq!(g_default.total_sum(), total, "default_config total_sum mismatch");
    assert_eq!(g_aggressive.total_sum(), total, "aggressive_config total_sum mismatch");

    assert_invariants(&g_default);
    assert_invariants(&g_aggressive);
}

// ── Regression: hand-crafted scenarios ───────────────────────────────────────

/// A single observation of known value: total_sum must equal that value,
/// range_sum(..) must equal it, and range_sum for non-covering range is ≥ 0.
#[test]
fn single_observation_known_value() {
    let mut g = GvGraph::<u64, u64, 8>::new(Config {
        split_threshold: 100, // high: no split, value stays at root
        depth_create: 3,
        depth_evict: 6,
        budget: None,
        alpha_relax: 0.75,
        bounded_eviction: false,
    });
    g.observe(128u64, 42u64);
    assert_eq!(g.total_sum(), 42);
    assert_eq!(g.range_sum(..), 42);
    // Range that fully misses coord 128 (but root still has own=42 pro-rated).
    assert!(g.range_sum(0u64..1u64) <= 42);
    assert!(g.range_sum(200u64..256u64) <= 42);
    // Full domain.
    assert_eq!(g.range_sum(0u64..256u64), 42);
    assert_invariants(&g);
}

/// Multiple known observations: naive total must match GvGraph total.
#[test]
fn ten_known_observations_total() {
    let observations: &[(u64, u64)] = &[
        (0, 1),
        (10, 2),
        (50, 3),
        (100, 4),
        (127, 5),
        (128, 6),
        (200, 7),
        (210, 8),
        (245, 9),
        (255, 10),
    ];
    let expected_total: u64 = observations.iter().map(|(_, d)| d).sum();

    let mut g = GvGraph::<u64, u64, 8>::new(default_config());
    let mut naive = VecAccumulator::new(256);
    for &(coord, delta) in observations {
        g.observe(coord, delta);
        naive.observe(coord, delta);
    }

    assert_eq!(g.total_sum(), expected_total);
    assert_eq!(g.total_sum(), naive.total_sum());
    // Right half (coords 128..256) holds observations 5..=10 → 6+7+8+9+10 = 40.
    let right_naive = naive.range_sum(128, 256);
    assert_eq!(right_naive, 40);
    // GvGraph full domain matches.
    assert_eq!(g.range_sum(..), expected_total);
    assert_invariants(&g);
}
