//! Plateau normalisation — consolidation and sum-recompute operations.
//!
//! These methods drive the normalisation pass that runs after any operation
//! that changes the shape of the G-tree (observe, decay, rebalance, evict).
//!
//! Execution order within a normalisation cycle:
//!   1. `plateau_recompute_sums`   — refresh sum values from the G-tree
//!   2. `normalize_plateaus`       — rebuild the plateau map and merge adjacent
//!                                   same-depth tiles
//!      ↳ `consolidate_all_basis`  — walk every basis element upward and merge
//!                                   sibling pairs into their parent
//!         ↳ `consolidate_basis_up` — the per-element upward walk

use crate::graph::{GvGraph, uniform_contour_depth_of};
use crate::handle::GNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    /// Walks upward from `gid`, merging sibling basis-element pairs into their
    /// parent whenever the subtree is uniform-depth.
    ///
    /// Visibility is `pub(super)` because [`place_basis_element`] in
    /// `plateau/mod.rs` calls this after placing a new basis element.
    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(clippy::too_many_lines)]
    pub(super) fn consolidate_basis_up(&mut self, mut gid: GNodeId) {
        use crate::nodes::gnode::GState;

        loop {
            let Some(parent_id) = self.gnodes.get(gid.index()).parent else {
                tracing::trace!(from = gid.index(), "consolidate_basis_up: stop — no parent");
                break;
            };
            if self.gnodes.get(parent_id.index()).state() != GState::Internal {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    parent_state = ?self.gnodes.get(parent_id.index()).state(),
                    "consolidate_basis_up: stop — parent not Internal",
                );
                break;
            }
            let (left, right) = {
                let pg = self.gnodes.get(parent_id.index());
                match (pg.left, pg.right) {
                    (Some(l), Some(r)) => (l, r),
                    _ => break,
                }
            };

            if self.gnodes.get(left.index()).state() == GState::SemiInternal
                || self.gnodes.get(right.index()).state() == GState::SemiInternal
            {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    "consolidate_basis_up: stop — semi-internal child"
                );
                break;
            }

            let Some(left_key) = self.plateau_basis.plateau_key(left) else {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    left = left.index(),
                    "consolidate_basis_up: stop — left not in basis"
                );
                break;
            };
            let Some(right_key) = self.plateau_basis.plateau_key(right) else {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    right = right.index(),
                    "consolidate_basis_up: stop — right not in basis"
                );
                break;
            };
            if left_key != right_key {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    ?left_key,
                    ?right_key,
                    "consolidate_basis_up: stop — children in different plateaus"
                );
                break;
            }

            let Some(uniform_depth) = uniform_contour_depth_of(&self.gnodes, parent_id, N) else {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    "consolidate_basis_up: stop — parent subtree not uniform"
                );
                break;
            };

            let Some(p) = self.plateaus.get(&left_key) else {
                break;
            };
            if p.depth != uniform_depth {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    plateau_depth = p.depth,
                    uniform_depth,
                    "consolidate_basis_up: stop — depth mismatch"
                );
                break;
            }

            tracing::debug!(
                left = left.index(),
                right = right.index(),
                parent = parent_id.index(),
                ?left_key,
                uniform_depth,
                "consolidate_basis_up: MERGING siblings into parent",
            );
            let key = left_key;
            self.plateau_basis.remove(left);
            self.plateau_basis.remove(right);
            self.plateau_basis.insert(key, parent_id);
            self.recompute_plateau(&key);

            gid = parent_id;
        }
    }

    /// Rebuilds the plateau map from scratch, merging all adjacent same-depth
    /// tiles, then consolidates up every basis element.
    ///
    /// Short-circuits when `plateaus_dirty` is not set.
    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(clippy::too_many_lines, clippy::float_cmp)]
    pub(crate) fn normalize_plateaus(&mut self) {
        use crate::nodes::gnode::GState;
        use crate::spatial::plateau::{BasisEdge, Plateau, basis_edge_of};
        use crate::tree::gtree::gnode_depth_from_interval;

        if !self.plateaus_dirty {
            return;
        }
        self.plateaus_dirty = false;

        let _span = tracing::debug_span!("normalize_plateaus").entered();

        #[cfg(debug_assertions)]
        let old_total: f64 = {
            let build = self.build_plateaus();
            build.values().map(|p| p.sum.to_f64_approx()).sum()
        };

        let mut elems: Vec<(GNodeId, BasisEdge<C>, u32, C, C, V)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let basis_ids: Vec<GNodeId> = self.plateau_basis.back_map().keys().copied().collect();
        for gid in basis_ids {
            let mut stack = vec![gid];
            while let Some(nid) = stack.pop() {
                if !seen.insert(nid) {
                    continue;
                }
                let g = self.gnodes.get(nid.index());
                match g.state() {
                    GState::Terminal => {
                        let depth = gnode_depth_from_interval(g.lo, g.hi, N);
                        elems.push((nid, basis_edge_of(g), depth, g.lo, g.hi, g.sum));
                    }
                    GState::SemiInternal => {
                        let depth = gnode_depth_from_interval(g.lo, g.hi, N);
                        elems.push((nid, basis_edge_of(g), depth, g.lo, g.hi, g.sum));

                        if let Some(left) = g.left {
                            stack.push(left);
                        }
                        if let Some(right) = g.right {
                            stack.push(right);
                        }
                    }
                    GState::Internal => {
                        if let Some(ud) = uniform_contour_depth_of(&self.gnodes, nid, N) {
                            elems.push((nid, basis_edge_of(g), ud, g.lo, g.hi, g.sum));
                        } else {
                            if let Some(left) = g.left {
                                stack.push(left);
                            }
                            if let Some(right) = g.right {
                                stack.push(right);
                            }
                        }
                    }
                }
            }
        }
        elems.sort_by_key(|e| e.1);

        #[cfg(debug_assertions)]
        {
            let mut ids: Vec<usize> = elems.iter().map(|e| e.0.index()).collect();
            ids.sort_unstable();
            for w in ids.windows(2) {
                debug_assert_ne!(
                    w[0], w[1],
                    "normalize_plateaus step 1: duplicate GNodeId({}) in DFS collection",
                    w[0],
                );
            }
        }

        let mut new_plateaus: std::collections::BTreeMap<BasisEdge<C>, Plateau<C, V>> =
            std::collections::BTreeMap::new();
        let mut assignments: Vec<(BasisEdge<C>, GNodeId)> = Vec::with_capacity(elems.len());

        for (gid, be, depth, lo, hi, sum) in &elems {
            let merge_key = new_plateaus
                .range(..*be)
                .next_back()
                .filter(|(_, p)| p.depth == *depth)
                .map(|(&k, _)| k);

            let key = if let Some(mk) = merge_key {
                let p = new_plateaus.get_mut(&mk).unwrap();
                if hi.total_cmp(&p.end) == std::cmp::Ordering::Greater {
                    p.end = *hi;
                }
                if lo.total_cmp(&p.start) == std::cmp::Ordering::Less {
                    p.start = *lo;
                }
                p.sum = V::add(p.sum, *sum);
                mk
            } else {
                new_plateaus.insert(
                    *be,
                    Plateau {
                        basis_edge: *be,
                        start: *lo,
                        end: *hi,
                        depth: *depth,
                        sum: *sum,
                    },
                );
                *be
            };
            assignments.push((key, *gid));
        }

        #[cfg(debug_assertions)]
        {
            let mut key_sums: std::collections::BTreeMap<BasisEdge<C>, f64> =
                std::collections::BTreeMap::new();
            for (i, (key, _gid)) in assignments.iter().enumerate() {
                *key_sums.entry(*key).or_default() += elems[i].5.to_f64_approx();
            }
            for (key, elem_sum) in &key_sums {
                let sweep_sum = new_plateaus[key].sum.to_f64_approx();
                debug_assert!(
                    sweep_sum == *elem_sum || (sweep_sum - elem_sum).abs() < 1e-9,
                    "normalize_plateaus step 2: sweep sum {sweep_sum} != \
                     element sum {elem_sum} for plateau {key:?}",
                );
            }
        }

        self.plateaus = new_plateaus;
        self.plateau_basis.rebuild(assignments);

        #[cfg(debug_assertions)]
        self.debug_check_plateau_sums("normalize step 3 (post-rebuild)");

        self.consolidate_all_basis();

        #[cfg(debug_assertions)]
        self.debug_check_plateau_sums("normalize step 4 (post-consolidate)");

        #[cfg(debug_assertions)]
        {
            let new_total: f64 = {
                let build = self.build_plateaus();
                build.values().map(|p| p.sum.to_f64_approx()).sum()
            };
            debug_assert!(
                old_total == new_total || (old_total - new_total).abs() < 1e-9,
                "normalize_plateaus: total plateau energy changed: \
                 old={old_total}, new={new_total}, delta={}",
                new_total - old_total,
            );
        }
    }

    /// Iterates every basis element and attempts to merge sibling pairs into
    /// their parent via [`consolidate_basis_up`].
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn consolidate_all_basis(&mut self) {
        let basis_snapshot: Vec<GNodeId> = self.plateau_basis.back_map().keys().copied().collect();
        let count = basis_snapshot.len();
        let mut merged = 0u32;
        for gid in basis_snapshot {
            if self.plateau_basis.plateau_key(gid).is_some() {
                let before = self.plateau_basis.basis_count();
                self.consolidate_basis_up(gid);
                let after = self.plateau_basis.basis_count();
                if after < before {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        merged += (before - after) as u32;
                    }
                }
            }
        }
        if merged > 0 {
            tracing::debug!(
                basis_before = count,
                basis_after = self.plateau_basis.basis_count(),
                merged,
                "consolidate_all_basis: completed",
            );
        }
    }

    /// Recomputes the `sum` field for every plateau entry from the current
    /// G-tree node sums.  Called after a bulk weight mutation (e.g. `decay`).
    #[cfg(feature = "dynamic-contour-tracking")]
    #[cfg_attr(not(debug_assertions), allow(unused_variables))]
    pub(crate) fn plateau_recompute_sums(&mut self, label: &str) {
        for (&key, plateau) in &mut self.plateaus {
            plateau.sum = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&r| self.gnodes.get(r.index()).sum)
                .fold(V::zero(), V::add);
        }

        #[cfg(debug_assertions)]
        for (&key, plateau) in &self.plateaus {
            let expected: V = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&r| self.gnodes.get(r.index()).sum)
                .fold(V::zero(), V::add);
            assert_eq!(
                plateau.sum, expected,
                "POST-{label}: plateau sum drift at key {key:?}"
            );
        }
    }
}
