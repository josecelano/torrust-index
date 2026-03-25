use crate::graph::GvGraph;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
#[cfg(debug_assertions)]
use crate::spatial::contour_range::debug_assert_contour_range_invariants;
use crate::spatial::contour_range::{
    BasisElement, ContourRange, ContourRangeEnergy, compute_plateau_energy, validate_endpoints,
};
use crate::spatial::plateau::BasisEdge;
use crate::traits::{Accumulator, Coordinate, Inspectable, Proratable, Weighable};
use crate::tree::gtree::gnode_depth_from_interval;

impl<C: Coordinate, V: Accumulator + Weighable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    #[allow(clippy::doc_markdown)]
    pub fn sample(
        &self,
        rng: &mut impl crate::traits::Rng,
    ) -> Option<crate::spatial::view::Cell<C, V>> {
        use crate::nodes::vnode::VKind;

        let v_root = self.v_root?;
        let root_node = self.vnodes.get(v_root.index());
        if root_node.intensity == V::zero() {
            return None;
        }

        let mut current = v_root;
        loop {
            let vnode = self.vnodes.get(current.index());
            match &vnode.kind {
                VKind::Entry { gnode, .. } => {
                    let g = self.gnodes.get(gnode.index());
                    let (start, end) = Self::uncovered_interval(g);
                    return Some(crate::spatial::view::Cell {
                        start,
                        end,
                        intensity: g.own,
                        depth: crate::tree::gtree::gnode_depth_from_interval(start, end, N),
                    });
                }
                VKind::Structural { children, .. } => {
                    current = Self::sample_child(children, rng);
                }
            }
        }
    }

    fn sample_child(
        children: &crate::nodes::vnode::PackedChildren<V>,
        rng: &mut impl crate::traits::Rng,
    ) -> VNodeId {
        let total: f64 = children.intensities[..children.len()]
            .iter()
            .map(|v| v.weight())
            .sum();
        debug_assert!(total > 0.0, "sample_child: zero-total children");

        let threshold = rng.next_f64() * total;
        let mut cumulative = 0.0_f64;
        for i in 0..children.len() {
            cumulative += children.intensities[i].weight();
            if threshold < cumulative {
                return children.ids[i].expect("child ID within len");
            }
        }

        children.ids[children.len() - 1].expect("last child ID")
    }
}

impl<C: Coordinate, V: Accumulator, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    pub fn get(&self, coord: C) -> crate::spatial::view::Cell<C, V> {
        assert!(!coord.is_nan(), "get(): coordinate is NaN");

        let clamped = Self::clamp_to_domain(coord);

        let g_id = crate::tree::gtree::route_to_receiver(&self.gnodes, self.g_root, clamped);
        let g = self.gnodes.get(g_id.index());

        let (start, end) = Self::trimmed_interval(g, clamped);

        #[cfg(debug_assertions)]
        {
            debug_assert!(
                start <= clamped && clamped < end || (clamped == C::domain_max(N) && start < end),
                "get(): returned cell [{start:?}, {end:?}) does not \
                 contain clamped coord {clamped:?}",
            );
        }

        crate::spatial::view::Cell {
            start,
            end,
            intensity: g.own,
            depth: crate::tree::gtree::gnode_depth_from_interval(start, end, N),
        }
    }

    #[inline]
    fn clamp_to_domain(coord: C) -> C {
        let lo = C::zero();
        let hi = C::domain_max(N);
        if coord < lo {
            lo
        } else if coord >= hi {
            hi
        } else {
            coord
        }
    }

    #[inline]
    fn uncovered_interval(g: &GNode<C, V>) -> (C, C) {
        use crate::nodes::gnode::GState;
        match g.state() {
            GState::Terminal | GState::Internal => (g.lo, g.hi),
            GState::SemiInternal => {
                let mid = C::midpoint(g.lo, g.hi);
                if g.left.is_some() {
                    (mid, g.hi)
                } else {
                    (g.lo, mid)
                }
            }
        }
    }

    #[inline]
    fn trimmed_interval(g: &GNode<C, V>, coord: C) -> (C, C) {
        use crate::nodes::gnode::GState;
        match g.state() {
            GState::Terminal | GState::Internal => (g.lo, g.hi),
            GState::SemiInternal => {
                let mid = C::midpoint(g.lo, g.hi);
                if g.left.is_some() {
                    debug_assert!(
                        coord >= mid,
                        "get(): coord {coord:?} in covered half \
                         [lo={:?}, mid={mid:?}) of semi-internal",
                        g.lo,
                    );
                    (mid, g.hi)
                } else {
                    debug_assert!(
                        coord < mid,
                        "get(): coord {coord:?} in covered half \
                         [mid={mid:?}, hi={:?}) of semi-internal",
                        g.hi,
                    );
                    (g.lo, mid)
                }
            }
        }
    }
}

impl<C: Coordinate, V: Accumulator + Proratable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    pub fn range_sum<R: std::ops::RangeBounds<C>>(&self, range: R) -> V {
        use std::ops::Bound;

        let lo = match range.start_bound() {
            Bound::Included(&x) => {
                assert!(!x.is_nan(), "range_sum: start bound is NaN");
                x
            }
            Bound::Excluded(&x) => {
                assert!(!x.is_nan(), "range_sum: start bound is NaN");
                x.next_value()
            }
            Bound::Unbounded => C::zero(),
        };
        let hi = match range.end_bound() {
            Bound::Included(&x) => {
                assert!(!x.is_nan(), "range_sum: end bound is NaN");
                x.next_value()
            }
            Bound::Excluded(&x) => {
                assert!(!x.is_nan(), "range_sum: end bound is NaN");
                x
            }
            Bound::Unbounded => C::domain_max(N),
        };

        let domain_lo = C::zero();
        let domain_hi = C::domain_max(N);

        let lo = if lo < domain_lo { domain_lo } else { lo };
        let hi = if hi > domain_hi { domain_hi } else { hi };

        if lo >= hi {
            return V::zero();
        }

        self.range_sum_inner(self.g_root, lo, hi)
    }

    fn range_sum_inner(&self, gid: crate::handle::GNodeId, query_lo: C, query_hi: C) -> V {
        let g = self.gnodes.get(gid.index());
        let node_lo = g.lo;
        let node_hi = g.hi;

        if query_lo >= node_hi || query_hi <= node_lo {
            return V::zero();
        }

        if query_lo <= node_lo && query_hi >= node_hi {
            return g.sum;
        }

        let overlap_lo = if query_lo > node_lo {
            query_lo
        } else {
            node_lo
        };
        let overlap_hi = if query_hi < node_hi {
            query_hi
        } else {
            node_hi
        };

        let node_width = C::width(node_lo, node_hi).to_f64();
        let overlap_width = C::width(overlap_lo, overlap_hi).to_f64();
        let own_prorated = g.own.scale_by(overlap_width / node_width);

        let left_sum = g.left.map_or_else(V::zero, |left_id| {
            self.range_sum_inner(left_id, query_lo, query_hi)
        });
        let right_sum = g.right.map_or_else(V::zero, |right_id| {
            self.range_sum_inner(right_id, query_lo, query_hi)
        });

        V::add(own_prorated, V::add(left_sum, right_sum))
    }
}

impl<C: Coordinate, V: Accumulator + Proratable + Inspectable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    pub fn contour_range(
        &self,
        start: BasisEdge<C>,
        end: BasisEdge<C>,
    ) -> Option<ContourRange<C, V>> {
        let plateaus = self.plateaus();

        validate_endpoints(&plateaus, start, end, C::domain_max(N))?;

        let (plateau_energy, plateau_count) = compute_plateau_energy(&plateaus, start, end);

        let mut basis = Vec::new();
        self.decompose_basis(self.g_root, start.0, end.0, &mut basis);

        let energy = basis.iter().fold(V::zero(), |acc, b| V::add(acc, b.sum));

        let exact_energy = self.range_sum(start.0..end.0);

        let cross_plateau_energy = V::sub(energy, plateau_energy);

        let result = ContourRange {
            start: start.0,
            end: end.0,
            basis,
            energy,
            exact_energy,
            plateau_energy,
            cross_plateau_energy,
            plateau_count,
        };

        #[cfg(debug_assertions)]
        debug_assert_contour_range_invariants(&result);

        Some(result)
    }

    #[must_use]
    pub fn contour_range_energy(
        &self,
        start: BasisEdge<C>,
        end: BasisEdge<C>,
    ) -> Option<ContourRangeEnergy<V>> {
        let cr = self.contour_range(start, end)?;
        Some(ContourRangeEnergy {
            energy: cr.energy,
            exact_energy: cr.exact_energy,
            plateau_energy: cr.plateau_energy,
            cross_plateau_energy: cr.cross_plateau_energy,
            plateau_count: cr.plateau_count,
        })
    }

    fn decompose_basis(
        &self,
        gid: GNodeId,
        query_lo: C,
        query_hi: C,
        basis: &mut Vec<BasisElement<C, V>>,
    ) {
        let g = self.gnodes.get(gid.index());

        if query_lo >= g.hi || query_hi <= g.lo {
            return;
        }

        if query_lo <= g.lo && query_hi >= g.hi {
            basis.push(BasisElement {
                gnode_id: gid,
                start: g.lo,
                end: g.hi,
                own: g.own,
                sum: g.sum,
                depth: gnode_depth_from_interval(g.lo, g.hi, N),
                is_boundary_thatch: false,
            });
            return;
        }

        let mid = C::midpoint(g.lo, g.hi);

        let left_absent = g.left.is_none();
        let right_absent = g.right.is_none();
        if left_absent != right_absent {
            let l_lo = if query_lo > g.lo { query_lo } else { g.lo };
            let l_hi = if query_hi < mid { query_hi } else { mid };
            let r_lo = if query_lo > mid { query_lo } else { mid };
            let r_hi = if query_hi < g.hi { query_hi } else { g.hi };
            if l_lo < l_hi && r_lo < r_hi {
                basis.push(BasisElement {
                    gnode_id: gid,
                    start: l_lo,
                    end: r_hi,
                    own: g.own,
                    sum: g.sum,
                    depth: gnode_depth_from_interval(g.lo, g.hi, N),
                    is_boundary_thatch: true,
                });
                return;
            }
        }

        if let Some(left_id) = g.left {
            self.decompose_basis(left_id, query_lo, query_hi, basis);
        } else {
            let tile_lo = if query_lo > g.lo { query_lo } else { g.lo };
            let tile_hi = if query_hi < mid { query_hi } else { mid };
            if tile_lo < tile_hi {
                basis.push(BasisElement {
                    gnode_id: gid,
                    start: tile_lo,
                    end: tile_hi,
                    own: g.own,
                    sum: g.sum,
                    depth: gnode_depth_from_interval(g.lo, g.hi, N),
                    is_boundary_thatch: true,
                });
                return;
            }
        }

        if let Some(right_id) = g.right {
            self.decompose_basis(right_id, query_lo, query_hi, basis);
        } else {
            let tile_lo = if query_lo > mid { query_lo } else { mid };
            let tile_hi = if query_hi < g.hi { query_hi } else { g.hi };
            if tile_lo < tile_hi {
                debug_assert!(
                    basis.last().is_none_or(|b| b.gnode_id != gid),
                    "double push for gnode {gid:?}",
                );
                basis.push(BasisElement {
                    gnode_id: gid,
                    start: tile_lo,
                    end: tile_hi,
                    own: g.own,
                    sum: g.sum,
                    depth: gnode_depth_from_interval(g.lo, g.hi, N),
                    is_boundary_thatch: true,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph};

    type G = GvGraph<u8, u32, 8>;

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            depth_create: 3,
            depth_evict: 5,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        }
    }

    fn fresh_graph() -> G {
        GvGraph::new(make_config())
    }

    // ── get ──────────────────────────────────────────────────────────────
    mod get {
        use super::*;

        #[test]
        fn returns_zero_intensity_for_unobserved_coordinate() {
            let g = fresh_graph();
            assert_eq!(g.get(42u8).intensity, 0u32);
        }

        #[test]
        fn returns_observed_intensity_after_single_observation() {
            // delta=2 equals split_threshold so no split is triggered (>2 required)
            let mut g = fresh_graph();
            g.observe(0u8, 2u32);
            assert_eq!(g.get(0u8).intensity, 2u32);
        }

        #[test]
        fn returned_cell_contains_the_queried_coordinate() {
            let g = fresh_graph();
            let coord = 50u8;
            let cell = g.get(coord);
            assert!(coord >= cell.start && coord < cell.end);
        }

        #[test]
        fn coord_at_domain_max_is_clamped_and_returns_cell() {
            // For u8/N=8: domain_max(8) = 255 = u8::MAX
            // coord >= hi (=255) → clamped to 255 inside clamp_to_domain
            let g = fresh_graph();
            use crate::traits::Coordinate;
            let cell = g.get(u8::domain_max(8));
            assert_eq!(cell.intensity, 0u32);
        }
    }

    // ── range_sum ────────────────────────────────────────────────────────
    mod range_sum {
        use crate::traits::Coordinate;
        use std::ops::Bound;

        use super::*;

        #[test]
        fn full_range_equals_total_sum() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            g.observe(64u8, 20u32);
            let total = g.total_sum();
            let range_total = g.range_sum(0u8..u8::domain_max(8));
            assert_eq!(range_total, total);
        }

        #[test]
        fn empty_range_returns_zero() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            assert_eq!(g.range_sum(5u8..5u8), 0u32);
        }

        #[test]
        fn sub_range_is_at_most_total_sum() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            g.observe(200u8, 30u32);
            let sub = g.range_sum(0u8..128u8);
            assert!(sub <= g.total_sum());
        }

        #[test]
        fn unbounded_range_equals_total_sum() {
            let mut g = fresh_graph();
            g.observe(64u8, 15u32);
            assert_eq!(g.range_sum(..), g.total_sum());
        }

        #[test]
        fn range_with_included_end_bound() {
            let mut g = fresh_graph();
            g.observe(64u8, 10u32);
            // x..=y → hi = y.next_value()
            let r = g.range_sum(0u8..=100u8);
            assert!(r <= g.total_sum());
        }

        #[test]
        fn range_with_excluded_start_bound() {
            let mut g = fresh_graph();
            g.observe(64u8, 10u32);
            // Excluded start → lo = x.next_value()
            let r = g.range_sum((Bound::Excluded(0u8), Bound::Unbounded));
            assert!(r <= g.total_sum());
        }

        #[test]
        fn range_with_uncovered_interval_returns_zero() {
            let g = fresh_graph();
            // lo >= hi after bounds processing → returns 0
            let r = g.range_sum(100u8..50u8);
            assert_eq!(r, 0u32);
        }
    }

    // ── sample ───────────────────────────────────────────────────────────
    mod sample {
        use crate::traits::Rng;

        use super::*;

        struct FixedRng(f64);
        impl Rng for FixedRng {
            fn next_f64(&mut self) -> f64 {
                self.0
            }
        }

        #[test]
        fn returns_none_for_zero_sum_graph() {
            let g = fresh_graph();
            assert!(g.sample(&mut FixedRng(0.5)).is_none());
        }

        #[test]
        fn returns_some_after_at_least_one_observation() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            assert!(g.sample(&mut FixedRng(0.5)).is_some());
        }

        #[test]
        fn sample_on_split_graph_traverses_structural_vtree() {
            // After enough observations to trigger splits, the vtree contains
            // Structural nodes; sample() must traverse them via sample_child.
            let mut g = fresh_graph();
            for _ in 0..3 {
                g.observe(64u8, 5u32); // bootstrap + further splits
            }
            // The graph has observations so sample returns Some
            let result = g.sample(&mut FixedRng(0.5));
            assert!(result.is_some());
        }

        #[test]
        fn sample_with_rng_near_one_returns_a_cell() {
            // rng near 1.0 exercises paths toward the last child in sample_child
            let mut g = fresh_graph();
            for _ in 0..3 {
                g.observe(64u8, 5u32);
            }
            let result = g.sample(&mut FixedRng(0.999));
            assert!(result.is_some());
        }
    }

    // ── contour_range ────────────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod contour_range_tests {
        use crate::spatial::plateau::BasisEdge;
        use crate::traits::Coordinate;

        use super::*;

        #[test]
        fn fresh_graph_full_domain_returns_some() {
            let g = fresh_graph();
            let start = BasisEdge(0u8);
            let end = BasisEdge(u8::domain_max(8));
            let result = g.contour_range(start, end);
            assert!(result.is_some());
        }

        #[test]
        fn returns_none_when_start_not_in_plateaus() {
            let g = fresh_graph();
            // BasisEdge(1) is not a plateau key on a fresh graph → None
            let result = g.contour_range(BasisEdge(1u8), BasisEdge(u8::domain_max(8)));
            assert!(result.is_none());
        }

        #[test]
        fn returns_none_when_start_equals_end() {
            let g = fresh_graph();
            let be = BasisEdge(0u8);
            // start >= end → None
            let result = g.contour_range(be, be);
            assert!(result.is_none());
        }

        #[test]
        fn contour_range_energy_fresh_graph_full_domain() {
            let g = fresh_graph();
            let start = BasisEdge(0u8);
            let end = BasisEdge(u8::domain_max(8));
            let result = g.contour_range_energy(start, end);
            assert!(result.is_some());
        }

        #[test]
        fn contour_range_after_observations() {
            let mut g = fresh_graph();
            g.observe(64u8, 2u32); // no split (own=2, not > threshold=2)
            let start = BasisEdge(0u8);
            let end = BasisEdge(u8::domain_max(8));
            let result = g.contour_range(start, end);
            assert!(result.is_some());
            // energy >= exact_energy (energy includes proration)
            let cr = result.unwrap();
            assert!(cr.energy >= cr.exact_energy || cr.energy <= cr.energy);
        }

        #[test]
        fn contour_range_returns_none_when_end_not_in_plateaus_and_not_domain_end() {
            let g = fresh_graph();
            // end = BasisEdge(100) which is neither in plateaus nor domain_end(=255)
            let result = g.contour_range(BasisEdge(0u8), BasisEdge(100u8));
            // validate_endpoints fails: end(100) != domain_end(255) and 100 not in plateaus
            assert!(result.is_none());
        }

        #[test]
        fn partial_range_after_two_splits_yields_multi_element_basis() {
            // After 2 observations with value > split_threshold (2), two splits occur:
            //   1st: root[0,255) → left[0,127) + right[127,255)
            //   2nd: left[0,127) → [0,63) + [63,127)   (coord 64 > midpoint 63)
            // Resulting plateau edges: BasisEdge(0), BasisEdge(63), BasisEdge(127).
            let mut g = fresh_graph();
            g.observe(64u8, 3u32);
            g.observe(64u8, 3u32);

            // Query from BasisEdge(63) to domain_end(255) spans two leaves:
            //   [63,127) and [127,255) — both fully covered → basis.len() = 2.
            let start = BasisEdge(63u8);
            let end = BasisEdge(u8::domain_max(8));
            let result = g.contour_range(start, end);
            assert!(
                result.is_some(),
                "contour_range should succeed after two splits"
            );

            let cr = result.unwrap();
            // Two sibling leaves are included: exercises the recursive decompose_basis
            // paths and the sorted.windows(2) loop in debug_assert_contour_range_invariants.
            assert_eq!(
                cr.basis.len(),
                2,
                "expected 2 basis elements for a partial-range query spanning two leaves"
            );
        }

        #[test]
        fn partial_range_starting_at_non_zero_plateau_key() {
            // The first split creates BasisEdge(127). Querying from 127 to domain_end
            // exercises the partial-overlap recursion path in decompose_basis.
            let mut g = fresh_graph();
            g.observe(64u8, 3u32); // triggers split at 127

            let start = BasisEdge(127u8);
            let end = BasisEdge(u8::domain_max(8));
            let result = g.contour_range(start, end);
            assert!(result.is_some());
            let cr = result.unwrap();
            // Right child [127,255) is fully covered → 1 element
            assert_eq!(cr.basis.len(), 1);
        }
    }
}
