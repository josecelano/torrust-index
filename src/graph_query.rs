#[cfg(debug_assertions)]
use crate::spatial::contour_range::debug_assert_contour_range_invariants;
use crate::spatial::contour_range::{BasisElement, ContourRange, ContourRangeEnergy, compute_plateau_energy, validate_endpoints};
use crate::nodes::gnode::GNode;
use crate::graph::GvGraph;
use crate::gtree::gnode_depth_from_interval;
use crate::handle::{GNodeId, VNodeId};
use crate::spatial::plateau::BasisEdge;
use crate::traits::{Accumulator, Coordinate, Inspectable, Proratable, Weighable};

impl<C: Coordinate, V: Accumulator + Weighable, const N: u32> GvGraph<C, V, N> {

    #[must_use]
    #[allow(clippy::doc_markdown)]
    pub fn sample(&self, rng: &mut impl crate::traits::Rng) -> Option<crate::spatial::view::Cell<C, V>> {
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
                        depth: crate::gtree::gnode_depth_from_interval(start, end, N),
                    });
                }
                VKind::Structural { children, .. } => {
                    current = Self::sample_child(children, rng);
                }
            }
        }
    }

    fn sample_child(children: &crate::nodes::vnode::PackedChildren<V>, rng: &mut impl crate::traits::Rng) -> VNodeId {
        let total: f64 = children.intensities[..children.len()].iter().map(|v| v.weight()).sum();
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

        let g_id = crate::gtree::route_to_receiver(&self.gnodes, self.g_root, clamped);
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
            depth: crate::gtree::gnode_depth_from_interval(start, end, N),
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
                if g.left.is_some() { (mid, g.hi) } else { (g.lo, mid) }
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

        let overlap_lo = if query_lo > node_lo { query_lo } else { node_lo };
        let overlap_hi = if query_hi < node_hi { query_hi } else { node_hi };

        let node_width = C::width(node_lo, node_hi).to_f64();
        let overlap_width = C::width(overlap_lo, overlap_hi).to_f64();
        let own_prorated = g.own.scale_by(overlap_width / node_width);

        let left_sum = g
            .left
            .map_or_else(V::zero, |left_id| self.range_sum_inner(left_id, query_lo, query_hi));
        let right_sum = g
            .right
            .map_or_else(V::zero, |right_id| self.range_sum_inner(right_id, query_lo, query_hi));

        V::add(own_prorated, V::add(left_sum, right_sum))
    }
}

impl<C: Coordinate, V: Accumulator + Proratable + Inspectable, const N: u32> GvGraph<C, V, N> {

    #[must_use]
    pub fn contour_range(&self, start: BasisEdge<C>, end: BasisEdge<C>) -> Option<ContourRange<C, V>> {

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
    pub fn contour_range_energy(&self, start: BasisEdge<C>, end: BasisEdge<C>) -> Option<ContourRangeEnergy<V>> {
        let cr = self.contour_range(start, end)?;
        Some(ContourRangeEnergy {
            energy: cr.energy,
            exact_energy: cr.exact_energy,
            plateau_energy: cr.plateau_energy,
            cross_plateau_energy: cr.cross_plateau_energy,
            plateau_count: cr.plateau_count,
        })
    }

    fn decompose_basis(&self, gid: GNodeId, query_lo: C, query_hi: C, basis: &mut Vec<BasisElement<C, V>>) {
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
