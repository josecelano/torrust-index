use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::graph::{GvGraph, uniform_contour_depth_of};
use crate::handle::GNodeId;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau::{BasisEdge, Plateau};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau_basis::PlateauBasis;
use crate::traits::{Accumulator, Coordinate, Inspectable};

mod noop;
mod normalise;

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        #[cfg(feature = "dynamic-contour-tracking")]
        {
            self.debug_assert_plateau_mirror_consistency("plateaus()");
            Cow::Borrowed(&self.plateaus)
        }
        #[cfg(not(feature = "dynamic-contour-tracking"))]
        {
            Cow::Owned(self.build_plateaus())
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn debug_assert_plateau_mirror_consistency(&self, label: &str) {
        if !cfg!(debug_assertions) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }

        let rebuilt = self.build_plateaus();
        if self.plateaus != rebuilt {
            let dyn_keys: std::collections::BTreeSet<_> = self.plateaus.keys().collect();
            let stat_keys: std::collections::BTreeSet<_> = rebuilt.keys().collect();
            let only_dynamic: Vec<_> = dyn_keys.difference(&stat_keys).collect();
            let only_static: Vec<_> = stat_keys.difference(&dyn_keys).collect();
            let both: Vec<_> = dyn_keys.intersection(&stat_keys).collect();
            let differing: Vec<_> = both
                .iter()
                .filter(|&&k| self.plateaus.get(k) != rebuilt.get(k))
                .collect();

            tracing::error!(
                label,
                only_dynamic = ?only_dynamic,
                only_static = ?only_static,
                differing = ?differing,
                "PLATEAU DIVERGENCE DETECTED",
            );

            for &&key in &only_dynamic {
                let p = &self.plateaus[key];
                let basis = self.plateau_basis.basis_elements(key);
                tracing::error!(
                    key = ?key,
                    depth = p.depth,
                    start = ?p.start,
                    end = ?p.end,
                    sum = ?p.sum,
                    basis = ?basis.iter().map(|g| g.index()).collect::<Vec<_>>(),
                    "DYNAMIC-ONLY plateau",
                );
                for &gid in basis {
                    if self.gnodes.is_occupied(gid.index()) {
                        let g = self.gnodes.get(gid.index());
                        tracing::error!(
                            gid = gid.index(),
                            state = ?g.state(),
                            lo = ?g.lo,
                            hi = ?g.hi,
                            sum = ?g.sum,
                            "  basis element",
                        );
                    }
                }
            }

            for &&key in &only_static {
                let p = &rebuilt[key];
                tracing::error!(
                    key = ?key,
                    depth = p.depth,
                    start = ?p.start,
                    end = ?p.end,
                    sum = ?p.sum,
                    "STATIC-ONLY plateau",
                );
            }

            for &&&key in &differing {
                let dyn_p = &self.plateaus[key];
                let stat_p = &rebuilt[key];
                let basis = self.plateau_basis.basis_elements(key);
                tracing::error!(
                    key = ?key,
                    dyn_depth = dyn_p.depth,
                    dyn_start = ?dyn_p.start,
                    dyn_end = ?dyn_p.end,
                    dyn_sum = ?dyn_p.sum,
                    stat_depth = stat_p.depth,
                    stat_start = ?stat_p.start,
                    stat_end = ?stat_p.end,
                    stat_sum = ?stat_p.sum,
                    basis = ?basis.iter().map(|g| g.index()).collect::<Vec<_>>(),
                    "DIFFERS",
                );
                for &gid in basis {
                    if self.gnodes.is_occupied(gid.index()) {
                        let g = self.gnodes.get(gid.index());
                        let ud = uniform_contour_depth_of(&self.gnodes, gid, N);
                        tracing::error!(
                            gid = gid.index(),
                            state = ?g.state(),
                            lo = ?g.lo,
                            hi = ?g.hi,
                            sum = ?g.sum,
                            uniform_depth = ?ud,
                            "  basis element",
                        );
                    }
                }
            }

            tracing::error!(dump = %crate::diagnostics::invariants::dump_gtree::<C, V, N>(self), "G-TREE DUMP");

            panic!(
                "{label}: dynamic-contour-tracking mirror diverged from static rebuild\n\
                 left (dynamic): {:#?}\n\
                 right (static): {:#?}",
                self.plateaus, rebuilt
            );
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn build_plateaus(&self) -> BTreeMap<BasisEdge<C>, Plateau<C, V>> {
        use crate::nodes::gnode::GState;
        use crate::spatial::plateau::basis_edge_of;
        use crate::tree::gtree::gnode_depth_from_interval;

        let mut basis: Vec<(BasisEdge<C>, u32, C, C, V)> = Vec::new();
        let mut stack = vec![self.g_root];
        while let Some(gid) = stack.pop() {
            let g = self.gnodes.get(gid.index());
            match g.state() {
                GState::Terminal => {
                    let depth = gnode_depth_from_interval(g.lo, g.hi, N);
                    basis.push((BasisEdge(g.lo), depth, g.lo, g.hi, g.sum));
                }
                GState::SemiInternal => {
                    let depth = gnode_depth_from_interval(g.lo, g.hi, N);
                    basis.push((basis_edge_of(g), depth, g.lo, g.hi, g.sum));

                    if let Some(left) = g.left {
                        stack.push(left);
                    }
                    if let Some(right) = g.right {
                        stack.push(right);
                    }
                }
                GState::Internal => {
                    if let Some(ud) = uniform_contour_depth_of(&self.gnodes, gid, N) {
                        basis.push((basis_edge_of(g), ud, g.lo, g.hi, g.sum));
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

    #[cfg(feature = "dynamic-contour-tracking")]
    #[must_use]
    #[inline]
    pub(crate) const fn plateau_basis(&self) -> &PlateauBasis<C> {
        &self.plateau_basis
    }

    #[doc(hidden)]
    #[cfg(feature = "dynamic-contour-tracking")]
    #[must_use]
    #[allow(clippy::type_complexity)]
    pub fn debug_plateau_basis(
        &self,
    ) -> Vec<(BasisEdge<C>, Vec<(usize, C, C, &'static str, u32)>)> {
        let mut result = Vec::new();
        for &key in self.plateaus.keys() {
            let elements = self.plateau_basis.basis_elements(&key);
            let infos: Vec<_> = elements
                .iter()
                .map(|&gid| {
                    let g = self.gnodes.get(gid.index());
                    let state_str = match g.state() {
                        crate::nodes::gnode::GState::Terminal => "Terminal",
                        crate::nodes::gnode::GState::Internal => "Internal",
                        crate::nodes::gnode::GState::SemiInternal => "SemiInternal",
                    };
                    let g_depth = crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N);
                    (gid.index(), g.lo, g.hi, state_str, g_depth)
                })
                .collect();
            result.push((key, infos));
        }
        result
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn recompute_plateau(&mut self, key: &BasisEdge<C>) {
        let elements = self.plateau_basis.basis_elements(key);
        if elements.is_empty() {
            return;
        }

        let first = *elements.iter().next().unwrap();
        let mut min_lo = self.gnodes.get(first.index()).lo;
        let mut max_hi = self.gnodes.get(first.index()).hi;
        let mut sum = V::zero();
        let mut depth = 0u32;

        for &gid in elements {
            let g = self.gnodes.get(gid.index());
            if g.lo.total_cmp(&min_lo) == std::cmp::Ordering::Less {
                min_lo = g.lo;
            }
            if g.hi.total_cmp(&max_hi) == std::cmp::Ordering::Greater {
                max_hi = g.hi;
            }
            sum = V::add(sum, g.sum);

            let d = match g.state() {
                crate::nodes::gnode::GState::Terminal
                | crate::nodes::gnode::GState::SemiInternal => {
                    crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N)
                }
                crate::nodes::gnode::GState::Internal => {
                    uniform_contour_depth_of(&self.gnodes, gid, N).unwrap_or_else(|| {
                        crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N) + 1
                    })
                }
            };
            depth = depth.max(d);
        }

        if let Some(p) = self.plateaus.get_mut(key) {
            p.start = min_lo;
            p.end = max_hi;
            p.sum = sum;
            p.depth = depth;
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(clippy::too_many_lines)]
    pub(crate) fn place_basis_element(&mut self, gnode: GNodeId, depth: u32) {
        use crate::spatial::plateau::{BasisEdge, Plateau, basis_edge_of};

        let g = self.gnodes.get(gnode.index());
        let key = basis_edge_of(g);
        let lo = g.lo;
        let hi = g.hi;
        let sum = g.sum;

        let left_key = self
            .plateaus
            .range(..key)
            .next_back()
            .filter(|(_, p)| p.depth == depth && p.end.total_cmp(&lo) != std::cmp::Ordering::Less)
            .map(|(&k, _)| k)
            .filter(|lk| {
                self.plateaus
                    .range((
                        std::ops::Bound::Excluded(*lk),
                        std::ops::Bound::Excluded(key),
                    ))
                    .next()
                    .is_none()
            });

        let right_key = self
            .plateaus
            .range(BasisEdge(hi)..)
            .next()
            .filter(|(_, p)| {
                p.depth == depth && p.start.total_cmp(&hi) != std::cmp::Ordering::Greater
            })
            .map(|(&k, _)| k)
            .filter(|rk| {
                self.plateaus
                    .range((
                        std::ops::Bound::Excluded(key),
                        std::ops::Bound::Excluded(*rk),
                    ))
                    .next()
                    .is_none()
            });

        match (left_key, right_key) {
            (Some(lk), Some(rk)) => {
                // ── Arm: merge-both ───────────────────────────────────────────────
                // gnode bridges two adjacent same-depth plateaus; merge them
                // into the left plateau and re-key all right members.
                self.plateau_basis.insert(lk, gnode);
                let rights: Vec<_> = self
                    .plateau_basis
                    .basis_elements(&rk)
                    .iter()
                    .copied()
                    .collect();
                for rid in rights {
                    self.plateau_basis.remove(rid);
                    self.plateau_basis.insert(lk, rid);
                }
                self.plateaus.remove(&rk);
                self.recompute_plateau(&lk);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?lk,
                    ?rk,
                    "place_basis_element: merge-both"
                );
            }
            (Some(lk), None) => {
                // ── Arm: insert-left ─────────────────────────────────────────────
                // gnode extends the left plateau rightward; add it and widen.
                self.plateau_basis.insert(lk, gnode);
                self.recompute_plateau(&lk);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?lk,
                    "place_basis_element: insert-left"
                );
            }
            (None, Some(rk)) => {
                // ── Arm: rekey-right ────────────────────────────────────────────
                // gnode starts a new plateau that the right plateau should merge
                // into; move all right members to the new key and remove the old
                // right entry.
                let rights: Vec<_> = self
                    .plateau_basis
                    .basis_elements(&rk)
                    .iter()
                    .copied()
                    .collect();
                for rid in rights {
                    self.plateau_basis.remove(rid);
                    self.plateau_basis.insert(key, rid);
                }
                self.plateau_basis.insert(key, gnode);
                self.plateaus.remove(&rk);
                self.plateaus.insert(
                    key,
                    Plateau {
                        basis_edge: key,
                        start: lo,
                        end: lo,
                        depth,
                        sum,
                    },
                );
                self.recompute_plateau(&key);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?rk,
                    "place_basis_element: rekey-right"
                );
            }
            (None, None) => {
                // ── Arm: new-plateau ─────────────────────────────────────────────
                // No adjacent same-depth plateau exists; create a fresh entry.
                self.plateau_basis.insert(key, gnode);
                if let std::collections::btree_map::Entry::Vacant(e) = self.plateaus.entry(key) {
                    e.insert(Plateau {
                        basis_edge: key,
                        start: lo,
                        end: hi,
                        depth,
                        sum,
                    });
                } else {
                    self.recompute_plateau(&key);
                }
                tracing::trace!(gnode = gnode.index(), "place_basis_element: new-plateau");
            }
        }

        self.consolidate_basis_up(gnode);

        if self.gnodes.get(gnode.index()).state() == crate::nodes::gnode::GState::SemiInternal {
            let final_key = self.plateau_basis.plateau_key(gnode).expect("just placed");
            self.pending_p_i4.push((gnode, final_key));
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn collect_subtree_basis_elements(
        &self,
        gid: GNodeId,
        out: &mut Vec<(GNodeId, u32)>,
    ) {
        use crate::nodes::gnode::GState;
        let g = self.gnodes.get(gid.index());
        match g.state() {
            GState::Terminal | GState::SemiInternal => {
                let depth = crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N);
                out.push((gid, depth));
            }
            GState::Internal => {
                if let Some(ud) = uniform_contour_depth_of(&self.gnodes, gid, N) {
                    out.push((gid, ud));
                } else {
                    if let Some(l) = g.left {
                        self.collect_subtree_basis_elements(l, out);
                    }
                    if let Some(r) = g.right {
                        self.collect_subtree_basis_elements(r, out);
                    }
                }
            }
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn place_sorted(&mut self, elements: &mut [(GNodeId, u32)]) {
        use crate::spatial::plateau::basis_edge_of;
        elements.sort_by(|a, b| {
            let a_key = basis_edge_of(self.gnodes.get(a.0.index()));
            let b_key = basis_edge_of(self.gnodes.get(b.0.index()));
            a_key.cmp(&b_key)
        });
        for &(gid, depth) in elements.iter() {
            self.place_basis_element(gid, depth);
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(clippy::float_cmp)]
    pub(crate) fn debug_check_plateau_sums(&self, label: &str) {
        if !cfg!(debug_assertions) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }

        for (&key, plateau) in &self.plateaus {
            let elems: Vec<_> = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&gid| {
                    let g = self.gnodes.get(gid.index());
                    (
                        gid.index(),
                        g.sum.to_f64_approx(),
                        format!("{:?}", g.state()),
                    )
                })
                .collect();
            let expected: f64 = elems.iter().map(|e| e.1).sum();
            let actual = plateau.sum.to_f64_approx();
            assert!(
                expected == actual || (expected - actual).abs() < 1e-9,
                "{label}: plateau sum mismatch at key {key:?}\n\
                 tracked={actual}, recomputed={expected}\n\
                 basis_elements={elems:?}",
            );
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn fixup_plateau(&mut self, old_key: crate::spatial::plateau::BasisEdge<C>) {
        use crate::spatial::plateau::{BasisEdge, Plateau, basis_edge_of};

        let element_ids: Vec<GNodeId> = self
            .plateau_basis
            .basis_elements(&old_key)
            .iter()
            .copied()
            .collect();

        if element_ids.is_empty() {
            self.plateaus.remove(&old_key);
            return;
        }

        if element_ids.len() > 1 {
            let mut intervals: Vec<(GNodeId, C, C)> = element_ids
                .iter()
                .map(|&gid| {
                    let g = self.gnodes.get(gid.index());
                    (gid, g.lo, g.hi)
                })
                .collect();
            intervals.sort_by(|a, b| a.1.total_cmp(&b.1));

            let contiguous = intervals
                .windows(2)
                .all(|w| w[0].2.total_cmp(&w[1].1) == std::cmp::Ordering::Equal);

            if !contiguous {
                tracing::debug!(
                    ?old_key,
                    n_elements = element_ids.len(),
                    "fixup_plateau: non-contiguous remainder, evacuating"
                );
                let mut displaced: Vec<(GNodeId, u32)> = Vec::with_capacity(element_ids.len());
                for &gid in &element_ids {
                    self.plateau_basis.remove(gid);
                    let g = self.gnodes.get(gid.index());
                    let d = match g.state() {
                        crate::nodes::gnode::GState::Terminal
                        | crate::nodes::gnode::GState::SemiInternal => {
                            crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N)
                        }
                        crate::nodes::gnode::GState::Internal => {
                            crate::graph::uniform_contour_depth_of(&self.gnodes, gid, N)
                                .unwrap_or_else(|| {
                                    crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N) + 1
                                })
                        }
                    };
                    displaced.push((gid, d));
                }
                self.plateaus.remove(&old_key);
                self.place_sorted(&mut displaced);
                return;
            }
        }

        let min_key: BasisEdge<C> = element_ids
            .iter()
            .map(|&gid| basis_edge_of(self.gnodes.get(gid.index())))
            .min()
            .unwrap();

        if min_key == old_key {
            self.recompute_plateau(&old_key);
        } else {
            for &eid in &element_ids {
                self.plateau_basis.remove(eid);
            }
            self.plateaus.remove(&old_key);
            for &eid in &element_ids {
                self.plateau_basis.insert(min_key, eid);
            }
            self.plateaus.insert(
                min_key,
                Plateau {
                    basis_edge: min_key,
                    start: C::zero(),
                    end: C::zero(),
                    depth: 0,
                    sum: V::zero(),
                },
            );
            self.recompute_plateau(&min_key);
        }
    }

    /// Restores the **P-I4 invariant**: every semi-internal G-node must belong
    /// to a *different* plateau than its single child.
    ///
    /// A G-node is *semi-internal* when it has exactly one occupied child slot
    /// (the other slot is empty — it covers the node's "uncovered" half).
    /// After a split or promote operation the semi-internal parent and its child
    /// may temporarily land in the same plateau.  This function detects those
    /// cases by scanning the basis for semi-internal nodes and, for each one
    /// whose child falls within the same plateau key, calls `split_for_p_i4`
    /// to relocate the child's boundary into a fresh plateau.
    ///
    /// It is called at the end of every mutating operation (after
    /// `normalize_plateaus`) to keep the plateau index consistent.
    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn repair_p_i4(&mut self) {
        use crate::nodes::gnode::GState;
        use crate::spatial::plateau::BasisEdge;

        let span = tracing::debug_span!(
            "repair_p_i4",
            pending = self.pending_p_i4.len(),
            candidates = tracing::field::Empty,
        )
        .entered();

        let mut candidates: Vec<(GNodeId, BasisEdge<C>)> = Vec::new();
        for (&key, elements) in self.plateau_basis.iter() {
            for &gid in elements {
                if self.gnodes.is_occupied(gid.index())
                    && self.gnodes.get(gid.index()).state() == GState::SemiInternal
                {
                    candidates.push((gid, key));
                }
            }
        }
        self.pending_p_i4.extend(candidates);
        span.record("candidates", self.pending_p_i4.len());

        while let Some((gid, pk)) = self.pending_p_i4.pop() {
            if self.plateau_basis.plateau_key(gid) != Some(pk) {
                continue;
            }
            if !self.gnodes.is_occupied(gid.index()) {
                continue;
            }
            let g = self.gnodes.get(gid.index());
            if g.state() != GState::SemiInternal {
                continue;
            }

            let Some(child_id) = g.left.or(g.right) else {
                continue;
            };
            let child_lo = self.gnodes.get(child_id.index()).lo;
            let child_pk = self
                .plateaus
                .range(..=BasisEdge(child_lo))
                .next_back()
                .map(|(&k, _)| k);

            if child_pk == Some(pk) {
                self.split_for_p_i4(pk, child_id);
            }
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    fn split_for_p_i4(
        &mut self,
        parent_pk: crate::spatial::plateau::BasisEdge<C>,
        child_id: GNodeId,
    ) {
        use crate::nodes::gnode::GState;
        use crate::spatial::plateau::{Plateau, basis_edge_of};

        let boundary_id = self.find_boundary_node(child_id, parent_pk);
        let Some(boundary_id) = boundary_id else {
            return;
        };

        let bg = self.gnodes.get(boundary_id.index());
        let boundary_key = basis_edge_of(bg);
        let boundary_depth = match bg.state() {
            GState::Terminal | GState::SemiInternal => {
                crate::tree::gtree::gnode_depth_from_interval(bg.lo, bg.hi, N)
            }
            GState::Internal => crate::tree::gtree::gnode_depth_from_interval(bg.lo, bg.hi, N) + 1,
        };
        let b_lo = bg.lo;
        let b_hi = bg.hi;
        let b_sum = bg.sum;

        if let Some(old_pk) = self.plateau_basis.remove(boundary_id) {
            self.fixup_plateau(old_pk);
        }

        self.plateau_basis.insert(boundary_key, boundary_id);
        if let std::collections::btree_map::Entry::Vacant(e) = self.plateaus.entry(boundary_key) {
            e.insert(Plateau {
                basis_edge: boundary_key,
                start: b_lo,
                end: b_hi,
                depth: boundary_depth,
                sum: b_sum,
            });
        }
        self.recompute_plateau(&boundary_key);
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    fn find_boundary_node(
        &self,
        gid: GNodeId,
        parent_pk: crate::spatial::plateau::BasisEdge<C>,
    ) -> Option<GNodeId> {
        use crate::spatial::plateau::basis_edge_of;

        let g = self.gnodes.get(gid.index());
        let key = basis_edge_of(g);
        if key > parent_pk {
            return Some(gid);
        }

        if let Some(left) = g.left {
            if let Some(found) = self.find_boundary_node(left, parent_pk) {
                return Some(found);
            }
        }
        if let Some(right) = g.right {
            if let Some(found) = self.find_boundary_node(right, parent_pk) {
                return Some(found);
            }
        }
        None
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn plateau_after_observe<O: crate::traits::Observation<V>>(
        &mut self,
        g_id: GNodeId,
        delta: O,
    ) {
        let value_v: V = O::accumulate(V::zero(), delta);
        let mut cur = Some(g_id);
        while let Some(id) = cur {
            if let Some(key) = self.plateau_basis.plateau_key(id) {
                if let Some(p) = self.plateaus.get_mut(&key) {
                    p.sum = V::add(p.sum, value_v);
                }
            }
            cur = self.gnodes.get(id.index()).parent;
        }

        #[cfg(debug_assertions)]
        {
            let mut cur = Some(g_id);
            while let Some(id) = cur {
                if let Some(key) = self.plateau_basis.plateau_key(id) {
                    let elems: Vec<_> = self
                        .plateau_basis
                        .basis_elements(&key)
                        .iter()
                        .map(|&r| {
                            let g = self.gnodes.get(r.index());
                            (r.index(), g.sum.to_f64_approx(), format!("{:?}", g.state()))
                        })
                        .collect();
                    let expected: V = self
                        .plateau_basis
                        .basis_elements(&key)
                        .iter()
                        .map(|&r| self.gnodes.get(r.index()).sum)
                        .fold(V::zero(), V::add);
                    assert_eq!(
                        self.plateaus[&key].sum,
                        expected,
                        "POST-OBSERVE: plateau sum drift at key {key:?}\n\
                         delta={}, g_id=G({}), cur_node=G({})\n\
                         basis_elements={elems:?}",
                        value_v.to_f64_approx(),
                        g_id.index(),
                        id.index(),
                    );
                }
                cur = self.gnodes.get(id.index()).parent;
            }
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn plateau_after_bootstrap_split(&mut self, g_id: GNodeId, left_id: GNodeId) {
        let _span = tracing::debug_span!(
            "plateau_after_bootstrap_split",
            g_id = g_id.index(),
            left = left_id.index(),
        )
        .entered();

        self.plateaus_dirty = true;

        let old_key = self
            .plateau_basis
            .remove(g_id)
            .expect("bootstrap_split: g_id must be a basis element");
        self.fixup_plateau(old_key);

        let right_id = self
            .gnodes
            .get(g_id.index())
            .right
            .expect("bootstrap_split: g_id must have a right child");

        let left_depth = crate::tree::gtree::gnode_depth_from_interval(
            self.gnodes.get(left_id.index()).lo,
            self.gnodes.get(left_id.index()).hi,
            N,
        );
        let right_depth = crate::tree::gtree::gnode_depth_from_interval(
            self.gnodes.get(right_id.index()).lo,
            self.gnodes.get(right_id.index()).hi,
            N,
        );

        if left_depth == right_depth {
            self.place_basis_element(g_id, left_depth);
        } else {
            tracing::debug!(
                g = g_id.index(),
                left = left_id.index(),
                right = right_id.index(),
                left_depth,
                right_depth,
                "bootstrap_split: unequal child depths, placing children separately"
            );
            self.place_basis_element(left_id, left_depth);
            self.place_basis_element(right_id, right_depth);
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn plateau_after_catalytic_split(&mut self, g_id: GNodeId, left_id: GNodeId) {
        let _span = tracing::debug_span!(
            "plateau_after_catalytic_split",
            g_id = g_id.index(),
            left = left_id.index(),
        )
        .entered();

        self.plateaus_dirty = true;

        let right_id = self
            .gnodes
            .get(g_id.index())
            .right
            .expect("catalytic_split: g_id must have a right child");

        let left_depth = crate::tree::gtree::gnode_depth_from_interval(
            self.gnodes.get(left_id.index()).lo,
            self.gnodes.get(left_id.index()).hi,
            N,
        );
        let right_depth = crate::tree::gtree::gnode_depth_from_interval(
            self.gnodes.get(right_id.index()).lo,
            self.gnodes.get(right_id.index()).hi,
            N,
        );

        // ── Phase 1: Locate the covering basis element for g_id ─────────────────
        // Either g_id is in the basis directly, or an ancestor is found by
        // walking the parent chain; path siblings become displaced.
        let (old_key, displaced) = if let Some(key) = self.plateau_basis.remove(g_id) {
            let co_members: Vec<GNodeId> = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .copied()
                .collect();
            for &m in &co_members {
                self.plateau_basis.remove(m);
            }
            (key, co_members)
        } else {
            let mut path: Vec<GNodeId> = vec![g_id];
            let mut parent_opt = self.gnodes.get(g_id.index()).parent;
            let mut result = None;

            while let Some(p_id) = parent_opt {
                if let Some(key) = self.plateau_basis.remove(p_id) {
                    let mut displaced: Vec<GNodeId> = Vec::new();
                    for &path_node in &path {
                        let par = self
                            .gnodes
                            .get(path_node.index())
                            .parent
                            .expect("path node must have a parent");
                        let pg = self.gnodes.get(par.index());
                        let sibling = if pg.left == Some(path_node) {
                            pg.right
                        } else {
                            pg.left
                        };
                        if let Some(sib_id) = sibling {
                            displaced.push(sib_id);
                        }
                    }

                    let co_members: Vec<GNodeId> = self
                        .plateau_basis
                        .basis_elements(&key)
                        .iter()
                        .copied()
                        .collect();
                    for &m in &co_members {
                        self.plateau_basis.remove(m);
                    }
                    displaced.extend(co_members);
                    result = Some((key, displaced));
                    break;
                }
                path.push(p_id);
                parent_opt = self.gnodes.get(p_id.index()).parent;
            }

            result.expect("catalytic_split: no basis element covers g_id")
        };

        // ── Phase 2: Fixup the vacated plateau key ───────────────────────────
        self.fixup_plateau(old_key);

        // ── Phase 3: Collect displaced elements and re-place ───────────────────
        let mut to_place = Vec::new();
        for &sib_id in &displaced {
            self.collect_subtree_basis_elements(sib_id, &mut to_place);
        }
        if left_depth == right_depth {
            to_place.push((g_id, left_depth));
        } else {
            to_place.push((left_id, left_depth));
            to_place.push((right_id, right_depth));
        }
        self.place_sorted(&mut to_place);
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(dead_code)]
    pub(crate) fn plateau_after_legacy_promote(&mut self, new_gid: GNodeId) {
        use crate::nodes::gnode::GState;

        let ng = self.gnodes.get(new_gid.index());
        let parent_id = ng.parent.expect("legacy_promote child must have a parent");
        let child_depth = crate::tree::gtree::gnode_depth_from_interval(ng.lo, ng.hi, N);

        let pg = self.gnodes.get(parent_id.index());
        let existing_child_id = if pg.left == Some(new_gid) {
            pg.right
        } else {
            pg.left
        };

        if let Some(old_key) = self.plateau_basis.remove(parent_id) {
            self.fixup_plateau(old_key);
        }

        let mut to_place = Vec::new();
        if let Some(ec_id) = existing_child_id {
            let ec = self.gnodes.get(ec_id.index());
            if ec.state() != GState::Internal && self.plateau_basis.plateau_key(ec_id).is_none() {
                let existing_depth = crate::tree::gtree::gnode_depth_from_interval(ec.lo, ec.hi, N);
                to_place.push((ec_id, existing_depth));
            }
        }
        to_place.push((new_gid, child_depth));
        self.place_sorted(&mut to_place);
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) fn plateau_after_legacy_promotes_batched(&mut self, new_gnodes: &[GNodeId]) {
        use crate::nodes::gnode::GState;

        if new_gnodes.is_empty() {
            return;
        }

        let _span = tracing::debug_span!(
            "plateau_after_legacy_promotes_batched",
            count = new_gnodes.len(),
        )
        .entered();

        self.plateaus_dirty = true;

        let mut to_place = Vec::new();

        for &new_gid in new_gnodes {
            let ng = self.gnodes.get(new_gid.index());
            let parent_id = ng.parent.expect("legacy_promote child must have a parent");
            let child_depth = crate::tree::gtree::gnode_depth_from_interval(ng.lo, ng.hi, N);

            let pg = self.gnodes.get(parent_id.index());
            let existing_child_id = if pg.left == Some(new_gid) {
                pg.right
            } else {
                pg.left
            };

            if let Some(old_key) = self.plateau_basis.remove(parent_id) {
                self.fixup_plateau(old_key);
            }

            if let Some(ec_id) = existing_child_id {
                let ec = self.gnodes.get(ec_id.index());
                if ec.state() != GState::Internal && self.plateau_basis.plateau_key(ec_id).is_none()
                {
                    let existing_depth =
                        crate::tree::gtree::gnode_depth_from_interval(ec.lo, ec.hi, N);
                    to_place.push((ec_id, existing_depth));
                }
            }
            to_place.push((new_gid, child_depth));
        }

        self.place_sorted(&mut to_place);
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    #[allow(clippy::too_many_lines)]
    pub(crate) fn plateau_after_evict(
        &mut self,
        gnode_id: GNodeId,
        parent_id: GNodeId,
        parent_state_after: crate::nodes::gnode::GState,
        parent_lo: C,
        parent_hi: C,
    ) {
        use crate::nodes::gnode::GState;
        let _span = tracing::debug_span!(
            "plateau_after_evict",
            gnode = gnode_id.index(),
            parent = parent_id.index(),
            ?parent_state_after,
        )
        .entered();

        // ── Phase 1: Remove evicted node and parent from plateau basis ────────────
        // Walk the ancestor chain if the parent is not directly in the basis.
        let evicted_key = self.plateau_basis.remove(gnode_id);

        let mut displaced: Vec<GNodeId> = Vec::new();
        let mut displaced_extra: Vec<(GNodeId, u32)> = Vec::new();

        let ancestor_key = if let Some(key) = self.plateau_basis.remove(parent_id) {
            if parent_state_after == GState::SemiInternal {
                let g = self.gnodes.get(parent_id.index());
                if let Some(sib) = g.left.or(g.right) {
                    if let Some(ok) = self.plateau_basis.remove(sib) {
                        self.fixup_plateau(ok);
                    }
                    displaced.push(sib);
                }
            }
            Some(key)
        } else {
            let mut path: Vec<GNodeId> = vec![parent_id];
            let mut cur = self.gnodes.get(parent_id.index()).parent;
            let mut found = None;

            let parent_key = {
                let pg = self.gnodes.get(parent_id.index());
                crate::spatial::plateau::basis_edge_of(pg)
            };

            while let Some(anc) = cur {
                if let Some(&anc_key) = self.plateau_basis.plateau_key(anc).as_ref() {
                    let next_key = self
                        .plateaus
                        .range((
                            std::ops::Bound::Excluded(anc_key),
                            std::ops::Bound::Unbounded,
                        ))
                        .next()
                        .map(|(&k, _)| k);
                    let tile_covers =
                        parent_key >= anc_key && next_key.is_none_or(|nk| parent_key < nk);

                    if tile_covers {
                        self.plateau_basis.remove(anc);

                        if parent_state_after == GState::SemiInternal {
                            let g = self.gnodes.get(parent_id.index());
                            if let Some(sib) = g.left.or(g.right) {
                                if let Some(ok) = self.plateau_basis.remove(sib) {
                                    self.fixup_plateau(ok);
                                }
                                displaced.push(sib);
                            }
                        }

                        for &path_node in &path {
                            let par = self
                                .gnodes
                                .get(path_node.index())
                                .parent
                                .expect("path node must have a parent");
                            let pg = self.gnodes.get(par.index());
                            let sibling = if pg.left == Some(path_node) {
                                pg.right
                            } else {
                                pg.left
                            };
                            if let Some(sib_id) = sibling {
                                if displaced.contains(&sib_id) {
                                    continue;
                                }
                                if let Some(ok) = self.plateau_basis.remove(sib_id) {
                                    self.fixup_plateau(ok);
                                }
                                displaced.push(sib_id);
                            }
                        }

                        found = Some(anc_key);
                        break;
                    }
                }
                path.push(anc);
                cur = self.gnodes.get(anc.index()).parent;
            }

            found
        };

        // ── Phase 2: Fixup displaced plateau keys ────────────────────────────────
        if let Some(ek) = evicted_key {
            self.fixup_plateau(ek);
        }
        if let Some(ak) = ancestor_key {
            if evicted_key != Some(ak) {
                self.fixup_plateau(ak);
            }
        }

        // ── Phase 3: Compute parent depth after eviction ─────────────────────────
        let parent_depth = match parent_state_after {
            GState::Terminal | GState::SemiInternal => {
                crate::tree::gtree::gnode_depth_from_interval(parent_lo, parent_hi, N)
            }
            GState::Internal => {
                unreachable!("evict_tip: parent cannot remain Internal after eviction")
            }
        };

        let parent_be = BasisEdge(parent_lo);

        {
            // ── Phase 4: Evacuate right-adjacent same-depth plateaus ─────────────
            let right_keys: Vec<BasisEdge<C>> = self
                .plateaus
                .range(BasisEdge(parent_hi)..)
                .take_while(|(_, p)| p.start.total_cmp(&parent_hi) != std::cmp::Ordering::Greater)
                .filter(|(_, p)| p.depth == parent_depth)
                .map(|(&k, _)| k)
                .collect();
            for rk in right_keys {
                let members: Vec<GNodeId> = self
                    .plateau_basis
                    .basis_elements(&rk)
                    .iter()
                    .copied()
                    .collect();
                if !members.is_empty() {
                    tracing::trace!(
                        ?rk,
                        parent_depth,
                        members = ?members.iter().map(|g| g.index()).collect::<Vec<_>>(),
                        "evacuating right-adjacent same-depth plateau for evict placement",
                    );
                    for &m in &members {
                        self.plateau_basis.remove(m);
                    }
                    self.plateaus.remove(&rk);
                    for &m in &members {
                        self.collect_subtree_basis_elements(m, &mut displaced_extra);
                    }
                }
            }

            // ── Phase 5: Evacuate left-adjacent same-depth plateaus ──────────────
            let left_keys: Vec<BasisEdge<C>> = self
                .plateaus
                .range(..parent_be)
                .rev()
                .take_while(|(_, p)| p.end.total_cmp(&parent_lo) != std::cmp::Ordering::Less)
                .filter(|(_, p)| p.depth == parent_depth)
                .map(|(&k, _)| k)
                .collect();
            for lk in left_keys {
                let members: Vec<GNodeId> = self
                    .plateau_basis
                    .basis_elements(&lk)
                    .iter()
                    .copied()
                    .collect();
                if !members.is_empty() {
                    tracing::trace!(
                        ?lk,
                        parent_depth,
                        members = ?members.iter().map(|g| g.index()).collect::<Vec<_>>(),
                        "evacuating left-adjacent same-depth plateau for evict placement",
                    );
                    for &m in &members {
                        self.plateau_basis.remove(m);
                    }
                    self.plateaus.remove(&lk);
                    for &m in &members {
                        self.collect_subtree_basis_elements(m, &mut displaced_extra);
                    }
                }
            }
        }

        // ── Phase 6: Collect displaced nodes and re-place ────────────────────────
        let mut to_place = Vec::new();
        to_place.push((parent_id, parent_depth));
        for &sib_id in &displaced {
            self.collect_subtree_basis_elements(sib_id, &mut to_place);
        }
        to_place.extend(displaced_extra);
        self.place_sorted(&mut to_place);
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
