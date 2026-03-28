//! Plateau management — thin wrappers and public API.
//!
//! The heavy incremental-tracking algorithms now live in
//! [`DynamicPlateauTracker`] (`dynamic_tracker.rs`).  This module exposes:
//!
//! * A generic `impl<C, V, N, T> GvGraph<C, V, N, T>` with thin wrapper
//!   methods that delegate to `self.tracker`.
//! * A concrete `impl GvGraph<C, V, N, DynamicPlateauTracker<C, V>>` for
//!   the debug / diagnostic methods that need direct tracker-field access.
//! * The public `build_plateaus` and `select_plateaus` utilities (both
//!   feature-independent).

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::traits::{Accumulator, Coordinate, Inspectable, PlateauTracking};
use crate::tree::gtree::GTree;

pub mod dynamic_tracker;
pub(crate) mod noop_tracker;

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(unused_imports)]
pub use dynamic_tracker::DynamicPlateauTracker;
#[allow(unused_imports)]
pub(crate) use noop_tracker::NoopPlateauTracker;

// ── Generic impl (all trackers) ───────────────────────────────────────────────

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32, T: PlateauTracking<C, V>>
    GvGraph<C, V, N, T>
{
    // ── Public read API ────────────────────────────────────────────────

    /// Returns the current plateau map.
    ///
    /// With `dynamic-contour-tracking` this returns a borrowed reference to
    /// the incrementally-maintained mirror; without the feature it rebuilds the
    /// map from the G-tree on every call.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        self.tracker.plateaus()
    }

    /// Rebuilds the plateau map by walking the G-tree statically.
    ///
    /// Used for consistency checks and as the implementation of `plateaus()`
    /// when `dynamic-contour-tracking` is disabled.
    #[doc(hidden)]
    #[must_use]
    pub fn build_plateaus(&self) -> BTreeMap<BasisEdge<C>, Plateau<C, V>> {
        use crate::nodes::gnode::GState;
        use crate::spatial::plateau::basis_edge_of;

        let mut basis: Vec<(BasisEdge<C>, u32, C, C, V)> = Vec::new();
        let mut stack = vec![self.gtree.root];
        while let Some(gid) = stack.pop() {
            let g = self.gtree.nodes.get(gid.index());
            match g.state() {
                GState::Terminal => {
                    let depth = GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi());
                    basis.push((BasisEdge(g.lo()), depth, g.lo(), g.hi(), g.sum()));
                }
                GState::SemiInternal => {
                    let depth = GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi());
                    basis.push((basis_edge_of(g), depth, g.lo(), g.hi(), g.sum()));

                    if let Some(left) = g.left() {
                        stack.push(left);
                    }
                    if let Some(right) = g.right() {
                        stack.push(right);
                    }
                }
                GState::Internal => {
                    if let Some(ud) = self.gtree.uniform_contour_depth(gid) {
                        basis.push((basis_edge_of(g), ud, g.lo(), g.hi(), g.sum()));
                    } else {
                        if let Some(left) = g.left() {
                            stack.push(left);
                        }
                        if let Some(right) = g.right() {
                            stack.push(right);
                        }
                    }
                }
            }
        }

        basis.sort_by_key(|a| a.0);

        let mut result: BTreeMap<BasisEdge<C>, Plateau<C, V>> = BTreeMap::new();
        for (be, depth, lo, hi, sum) in basis {
            let merged = if let Some((_, prev)) = result.iter_mut().next_back() {
                if prev.depth == depth {
                    if hi.total_cmp(&prev.end) == std::cmp::Ordering::Greater {
                        prev.end = hi;
                    }
                    if lo.total_cmp(&prev.start) == std::cmp::Ordering::Less {
                        prev.start = lo;
                    }
                    prev.sum = V::add(prev.sum, sum);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                result.insert(
                    be,
                    Plateau {
                        basis_edge: be,
                        start: lo,
                        end: hi,
                        depth,
                        sum,
                    },
                );
            }
        }

        result
    }

    #[must_use]
    pub fn select_plateaus(&self, lo: C, hi: C) -> Option<(BasisEdge<C>, BasisEdge<C>)> {
        use std::ops::Bound;

        if lo >= hi {
            return None;
        }

        let plateaus = self.plateaus();
        if plateaus.is_empty() {
            return None;
        }

        let start_entry = plateaus
            .range(..=BasisEdge(lo))
            .next_back()
            .map(|(k, _)| *k)?;

        let last_plateau_key = plateaus
            .range(..BasisEdge(hi))
            .next_back()
            .map(|(k, _)| *k)?;

        let end = plateaus
            .range((Bound::Excluded(last_plateau_key), Bound::Unbounded))
            .next()
            .map_or_else(|| BasisEdge(C::domain_max(N)), |(k, _)| *k);

        if start_entry >= end {
            return None;
        }

        Some((start_entry, end))
    }

    // ── Thin wrappers ─────────────────────────────────────────────────

    pub(crate) fn plateau_after_observe<O: crate::traits::Observation<V>>(
        &mut self,
        g_id: GNodeId,
        delta: O,
    ) {
        let value_v: V = O::accumulate(V::zero(), delta);
        self.tracker.on_observe(&self.gtree.nodes, g_id, value_v);
    }

    pub(crate) fn plateau_after_bootstrap_split(&mut self, g_id: GNodeId, left_id: GNodeId) {
        self.tracker
            .on_bootstrap_split(&self.gtree.nodes, g_id, left_id);
    }

    pub(crate) fn plateau_after_catalytic_split(&mut self, g_id: GNodeId, left_id: GNodeId) {
        self.tracker
            .on_catalytic_split(&self.gtree.nodes, g_id, left_id);
    }

    pub(crate) fn plateau_after_legacy_promotes_batched(&mut self, new_gnodes: &[GNodeId]) {
        self.tracker
            .on_legacy_promotes_batched(&self.gtree.nodes, new_gnodes);
    }

    pub(crate) fn plateau_after_evict(
        &mut self,
        gnode_id: GNodeId,
        parent_id: GNodeId,
        parent_state_after: crate::nodes::gnode::GState,
        parent_lo: C,
        parent_hi: C,
    ) {
        self.tracker.on_evict(
            &self.gtree.nodes,
            gnode_id,
            parent_id,
            parent_state_after,
            parent_lo,
            parent_hi,
        );
    }

    pub(crate) fn normalize_plateaus(&mut self) {
        self.tracker.normalize(&self.gtree.nodes);
    }

    pub(crate) fn repair_p_i4(&mut self) {
        self.tracker.repair_p_i4(&self.gtree.nodes);
    }

    pub(crate) fn plateau_recompute_sums(&mut self, label: &str) {
        self.tracker.recompute_sums(&self.gtree.nodes, label);
    }

    /// Verifies that the plateau mirror is consistent with a fresh G-tree
    /// rebuild.  A no-op with `NoopPlateauTracker`; panics on divergence with
    /// `DynamicPlateauTracker`.  Called at algorithmic debug checkpoints.
    pub(crate) fn debug_assert_plateau_mirror_consistency(&self, label: &str) {
        if !cfg!(debug_assertions) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let fresh = self.build_plateaus();
        self.tracker
            .debug_assert_mirror_consistency(&self.gtree.nodes, &fresh, label);
    }
}

// ── Concrete impl for DynamicPlateauTracker — debug / diagnostic API ──────────

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32>
    GvGraph<C, V, N, DynamicPlateauTracker<C, V>>
{
    #[must_use]
    #[inline]
    pub(crate) const fn plateau_basis(&self) -> &crate::spatial::plateau_basis::PlateauBasis<C> {
        &self.tracker.plateau_basis
    }

    #[allow(clippy::float_cmp)]
    pub(crate) fn debug_check_plateau_sums(&self, label: &str) {
        self.tracker.debug_check_sums(&self.gtree.nodes, label);
    }

    #[doc(hidden)]
    #[must_use]
    #[allow(clippy::type_complexity)]
    pub fn debug_plateau_basis(
        &self,
    ) -> Vec<(BasisEdge<C>, Vec<(usize, C, C, &'static str, u32)>)> {
        let mut result = Vec::new();
        for &key in self.tracker.plateaus.keys() {
            let elements = self.tracker.plateau_basis.basis_elements(&key);
            let infos: Vec<_> = elements
                .iter()
                .map(|&gid| {
                    let g = self.gtree.nodes.get(gid.index());
                    let state_str = match g.state() {
                        crate::nodes::gnode::GState::Terminal => "Terminal",
                        crate::nodes::gnode::GState::Internal => "Internal",
                        crate::nodes::gnode::GState::SemiInternal => "SemiInternal",
                    };
                    let g_depth = GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi());
                    (gid.index(), g.lo(), g.hi(), state_str, g_depth)
                })
                .collect();
            result.push((key, infos));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph, StructuralConfig};
    use crate::spatial::plateau::BasisEdge;

    type G = GvGraph<u8, u32, 8>;

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            structural: StructuralConfig {
                depth_create: 3,
                depth_evict: 5,
                budget: None,
                alpha_relax: 0.5,
                bounded_eviction: false,
            },
        }
    }

    fn fresh() -> G {
        GvGraph::new(make_config())
    }

    // ── build_plateaus ────────────────────────────────────────────────
    mod build_plateaus_fn {
        use super::*;

        #[test]
        fn fresh_graph_has_exactly_one_plateau() {
            let g = fresh();
            let p = g.build_plateaus();
            assert_eq!(p.len(), 1);
        }

        #[test]
        fn fresh_plateau_sum_is_zero() {
            let g = fresh();
            let p = g.build_plateaus();
            let total: u32 = p.values().map(|pl| pl.sum).sum();
            assert_eq!(total, 0u32);
        }

        #[test]
        fn plateau_sum_equals_total_sum_after_observations() {
            let mut g = fresh();
            g.observe(32u8, 3u32);
            g.observe(192u8, 5u32);
            let expected = g.total_sum();
            let p = g.build_plateaus();
            let total: u32 = p.values().map(|pl| pl.sum).sum();
            assert_eq!(total, expected);
        }

        #[test]
        fn plateau_count_grows_after_splits() {
            let mut g = fresh();
            let before = g.build_plateaus().len();
            g.observe(64u8, 3u32); // bootstrap split
            let after = g.build_plateaus().len();
            assert!(after >= before, "split must not reduce plateau count");
        }
    }

    // ── plateaus (public accessor) ────────────────────────────────────
    mod plateaus_fn {
        use super::*;

        #[test]
        fn plateaus_is_consistent_with_build_plateaus() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            // Without dynamic-contour-tracking, plateaus() == build_plateaus().
            assert_eq!(g.plateaus().len(), g.build_plateaus().len());
        }
    }

    // ── select_plateaus ───────────────────────────────────────────────
    mod select_plateaus_fn {
        use super::*;

        #[test]
        fn returns_none_when_lo_equals_hi() {
            let g = fresh();
            assert!(g.select_plateaus(64u8, 64u8).is_none());
        }

        #[test]
        fn returns_none_when_lo_greater_than_hi() {
            let g = fresh();
            assert!(g.select_plateaus(200u8, 10u8).is_none());
        }

        #[test]
        fn returns_some_for_valid_range_in_observed_graph() {
            let mut g = fresh();
            g.observe(32u8, 3u32); // bootstrap split creates two plateaus
            // Select a sub-range that lies within one side
            let result = g.select_plateaus(0u8, 128u8);
            assert!(result.is_some(), "must find a covering plateau pair");
        }

        #[test]
        fn start_key_is_at_most_end_key() {
            let mut g = fresh();
            g.observe(32u8, 3u32);
            if let Some((start, end)) = g.select_plateaus(0u8, 128u8) {
                assert!(start <= end);
            }
        }

        #[test]
        fn start_key_encodes_lo_of_covering_plateau() {
            let mut g = fresh();
            g.observe(64u8, 3u32); // bootstrap: plateaus at 0 and 128
            // Asking for range [0, 64) must be covered by the first plateau
            let result = g.select_plateaus(0u8, 64u8);
            assert!(result.is_some());
            let (start, _end) = result.unwrap();
            assert_eq!(start, BasisEdge(0u8));
        }
    }
}
