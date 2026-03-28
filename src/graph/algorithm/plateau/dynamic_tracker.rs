//! Incremental plateau-mirror tracker for `dynamic-contour-tracking` builds.
//!
//! [`DynamicPlateauTracker`] owns all plateau-related state and all plateau
//! algorithms that were previously scattered across `GvGraph` as
//! `#[cfg(feature = "dynamic-contour-tracking")]` fields and `impl` blocks.
//!
//! It implements the [`PlateauTracking`] strategy trait and is threaded into
//! `GvGraph` as a type parameter (Phase 7 Step 7.3).

#![cfg(feature = "dynamic-contour-tracking")]

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::arena::Arena;
use crate::handle::GNodeId;
use crate::nodes::gnode::{GNode, GState};
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::spatial::plateau_basis::PlateauBasis;
use crate::traits::{Accumulator, Coordinate, PlateauTracking};
use crate::tree::gtree::{gnode_depth_from_interval, uniform_contour_depth_of};

/// Incrementally maintains a mirror of the plateau structure as observations,
/// splits, and evictions mutate the G-tree.
///
/// All heavyweight plateau logic previously lived on `GvGraph` as `impl` blocks
/// in `plateau/mod.rs` and `plateau/normalise.rs`.  This struct now owns both
/// the *state* and the *algorithms* for plateau tracking.
#[derive(Debug, Clone)]
pub struct DynamicPlateauTracker<C: Coordinate, V: Accumulator> {
    /// The plateau mirror: maps each basis edge to its current plateau.
    pub(crate) plateaus: BTreeMap<BasisEdge<C>, Plateau<C, V>>,

    /// Pending (semi-internal node, plateau key) pairs that must be checked by
    /// the P-I4 repair pass.
    pub(crate) pending_p_i4: Vec<(GNodeId, BasisEdge<C>)>,

    /// Reverse index: maps each G-node id to the basis-edge key of the plateau
    /// it contributes to.
    pub(crate) plateau_basis: PlateauBasis<C>,

    /// True when at least one mutating operation has occurred since the last
    /// `normalize` call.
    pub(crate) plateaus_dirty: bool,

    /// The coordinate bit-width (`N` from `GvGraph<C, V, N>`), stored at
    /// runtime so depth computations do not require the const generic here.
    pub(crate) n_bits: u32,
}

impl<C: Coordinate, V: Accumulator> DynamicPlateauTracker<C, V> {
    /// Creates a tracker pre-populated with the single root plateau.
    #[must_use]
    pub(crate) fn with_root(
        root_key: BasisEdge<C>,
        root_depth: u32,
        root_lo: C,
        root_hi: C,
        g_root: GNodeId,
        n_bits: u32,
    ) -> Self {
        let mut pb = PlateauBasis::new();
        pb.insert(root_key, g_root);
        let mut map = BTreeMap::new();
        map.insert(
            root_key,
            Plateau {
                basis_edge: root_key,
                start: root_lo,
                end: root_hi,
                depth: root_depth,
                sum: V::zero(),
            },
        );
        Self {
            plateaus: map,
            pending_p_i4: Vec::new(),
            plateau_basis: pb,
            plateaus_dirty: false,
            n_bits,
        }
    }

    // ── Internal helper methods ───────────────────────────────────────────────

    fn recompute_plateau(&mut self, gnodes: &Arena<GNode<C, V>>, key: &BasisEdge<C>) {
        let elements = self.plateau_basis.basis_elements(key);
        if elements.is_empty() {
            return;
        }

        let first = *elements.iter().next().unwrap();
        let mut min_lo = gnodes.get(first.index()).lo();
        let mut max_hi = gnodes.get(first.index()).hi();
        let mut sum = V::zero();
        let mut depth = 0u32;

        for &gid in elements {
            let g = gnodes.get(gid.index());
            if g.lo().total_cmp(&min_lo) == std::cmp::Ordering::Less {
                min_lo = g.lo();
            }
            if g.hi().total_cmp(&max_hi) == std::cmp::Ordering::Greater {
                max_hi = g.hi();
            }
            sum = V::add(sum, g.sum());

            let d = match g.state() {
                GState::Terminal | GState::SemiInternal => {
                    gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits)
                }
                GState::Internal => uniform_contour_depth_of(gnodes, gid, self.n_bits)
                    .unwrap_or_else(|| gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits) + 1),
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

    #[allow(clippy::too_many_lines)]
    fn place_basis_element(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        gnode: GNodeId,
        depth: u32,
    ) {
        use crate::spatial::plateau::{BasisEdge, Plateau, basis_edge_of};

        let g = gnodes.get(gnode.index());
        let key = basis_edge_of(g);
        let lo = g.lo();
        let hi = g.hi();
        let sum = g.sum();

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
                self.recompute_plateau(gnodes, &lk);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?lk,
                    ?rk,
                    "place_basis_element: merge-both"
                );
            }
            (Some(lk), None) => {
                self.plateau_basis.insert(lk, gnode);
                self.recompute_plateau(gnodes, &lk);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?lk,
                    "place_basis_element: insert-left"
                );
            }
            (None, Some(rk)) => {
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
                self.recompute_plateau(gnodes, &key);
                tracing::trace!(
                    gnode = gnode.index(),
                    ?rk,
                    "place_basis_element: rekey-right"
                );
            }
            (None, None) => {
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
                    self.recompute_plateau(gnodes, &key);
                }
                tracing::trace!(gnode = gnode.index(), "place_basis_element: new-plateau");
            }
        }

        self.consolidate_basis_up(gnodes, gnode);

        if gnodes.get(gnode.index()).state() == GState::SemiInternal {
            let final_key = self.plateau_basis.plateau_key(gnode).expect("just placed");
            self.pending_p_i4.push((gnode, final_key));
        }
    }

    fn collect_subtree_basis_elements(
        &self,
        gnodes: &Arena<GNode<C, V>>,
        gid: GNodeId,
        out: &mut Vec<(GNodeId, u32)>,
    ) {
        let g = gnodes.get(gid.index());
        match g.state() {
            GState::Terminal | GState::SemiInternal => {
                let depth = gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits);
                out.push((gid, depth));
            }
            GState::Internal => {
                if let Some(ud) = uniform_contour_depth_of(gnodes, gid, self.n_bits) {
                    out.push((gid, ud));
                } else {
                    if let Some(l) = g.left() {
                        self.collect_subtree_basis_elements(gnodes, l, out);
                    }
                    if let Some(r) = g.right() {
                        self.collect_subtree_basis_elements(gnodes, r, out);
                    }
                }
            }
        }
    }

    fn place_sorted(&mut self, gnodes: &Arena<GNode<C, V>>, elements: &mut [(GNodeId, u32)]) {
        use crate::spatial::plateau::basis_edge_of;
        elements.sort_by(|a, b| {
            let a_key = basis_edge_of(gnodes.get(a.0.index()));
            let b_key = basis_edge_of(gnodes.get(b.0.index()));
            a_key.cmp(&b_key)
        });
        for &(gid, depth) in elements.iter() {
            self.place_basis_element(gnodes, gid, depth);
        }
    }

    fn fixup_plateau(&mut self, gnodes: &Arena<GNode<C, V>>, old_key: BasisEdge<C>) {
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
                    let g = gnodes.get(gid.index());
                    (gid, g.lo(), g.hi())
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
                    let g = gnodes.get(gid.index());
                    let d = match g.state() {
                        GState::Terminal | GState::SemiInternal => {
                            gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits)
                        }
                        GState::Internal => uniform_contour_depth_of(gnodes, gid, self.n_bits)
                            .unwrap_or_else(|| {
                                gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits) + 1
                            }),
                    };
                    displaced.push((gid, d));
                }
                self.plateaus.remove(&old_key);
                self.place_sorted(gnodes, &mut displaced);
                return;
            }
        }

        let min_key: BasisEdge<C> = element_ids
            .iter()
            .map(|&gid| basis_edge_of(gnodes.get(gid.index())))
            .min()
            .unwrap();

        if min_key == old_key {
            self.recompute_plateau(gnodes, &old_key);
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
            self.recompute_plateau(gnodes, &min_key);
        }
    }

    fn split_for_p_i4(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        parent_pk: BasisEdge<C>,
        child_id: GNodeId,
    ) {
        use crate::spatial::plateau::{Plateau, basis_edge_of};

        let boundary_id = self.find_boundary_node(gnodes, child_id, parent_pk);
        let Some(boundary_id) = boundary_id else {
            return;
        };

        let bg = gnodes.get(boundary_id.index());
        let boundary_key = basis_edge_of(bg);
        let boundary_depth = match bg.state() {
            GState::Terminal | GState::SemiInternal => {
                gnode_depth_from_interval(bg.lo(), bg.hi(), self.n_bits)
            }
            GState::Internal => gnode_depth_from_interval(bg.lo(), bg.hi(), self.n_bits) + 1,
        };
        let b_lo = bg.lo();
        let b_hi = bg.hi();
        let b_sum = bg.sum();

        if let Some(old_pk) = self.plateau_basis.remove(boundary_id) {
            self.fixup_plateau(gnodes, old_pk);
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
        self.recompute_plateau(gnodes, &boundary_key);
    }

    fn find_boundary_node(
        &self,
        gnodes: &Arena<GNode<C, V>>,
        gid: GNodeId,
        parent_pk: BasisEdge<C>,
    ) -> Option<GNodeId> {
        use crate::spatial::plateau::basis_edge_of;

        let g = gnodes.get(gid.index());
        let key = basis_edge_of(g);
        if key > parent_pk {
            return Some(gid);
        }

        if let Some(left) = g.left() {
            if let Some(found) = self.find_boundary_node(gnodes, left, parent_pk) {
                return Some(found);
            }
        }
        if let Some(right) = g.right() {
            if let Some(found) = self.find_boundary_node(gnodes, right, parent_pk) {
                return Some(found);
            }
        }
        None
    }

    /// Walks upward from `gid`, merging sibling basis-element pairs into their
    /// parent whenever the subtree is uniform-depth.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn consolidate_basis_up(&mut self, gnodes: &Arena<GNode<C, V>>, mut gid: GNodeId) {
        loop {
            let Some(parent_id) = gnodes.get(gid.index()).parent() else {
                tracing::trace!(from = gid.index(), "consolidate_basis_up: stop — no parent");
                break;
            };
            if gnodes.get(parent_id.index()).state() != GState::Internal {
                tracing::trace!(
                    from = gid.index(),
                    parent = parent_id.index(),
                    parent_state = ?gnodes.get(parent_id.index()).state(),
                    "consolidate_basis_up: stop — parent not Internal",
                );
                break;
            }
            let (left, right) = {
                let pg = gnodes.get(parent_id.index());
                match (pg.left(), pg.right()) {
                    (Some(l), Some(r)) => (l, r),
                    _ => break,
                }
            };

            if gnodes.get(left.index()).state() == GState::SemiInternal
                || gnodes.get(right.index()).state() == GState::SemiInternal
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

            let Some(uniform_depth) = uniform_contour_depth_of(gnodes, parent_id, self.n_bits)
            else {
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
            self.recompute_plateau(gnodes, &key);

            gid = parent_id;
        }
    }

    /// Iterates every basis element and attempts to merge sibling pairs into
    /// their parent via [`consolidate_basis_up`].
    pub(crate) fn consolidate_all_basis(&mut self, gnodes: &Arena<GNode<C, V>>) {
        let basis_snapshot: Vec<GNodeId> = self.plateau_basis.back_map().keys().copied().collect();
        let count = basis_snapshot.len();
        let mut merged = 0u32;
        for gid in basis_snapshot {
            if self.plateau_basis.plateau_key(gid).is_some() {
                let before = self.plateau_basis.basis_count();
                self.consolidate_basis_up(gnodes, gid);
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
}

// ── Inspectable-bounded debug helpers ────────────────────────────────────────

impl<C: Coordinate, V: Accumulator + crate::traits::Inspectable> DynamicPlateauTracker<C, V> {
    #[allow(clippy::float_cmp)]
    pub(crate) fn debug_check_sums(&self, gnodes: &Arena<GNode<C, V>>, label: &str) {
        if !cfg!(debug_assertions) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }

        for (&key, plateau) in &self.plateaus {
            let elems: Vec<_> = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&gid| {
                    let g = gnodes.get(gid.index());
                    (
                        gid.index(),
                        g.sum().to_f64_approx(),
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
}

// ── PlateauTracking impl ─────────────────────────────────────────────────────

impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V> for DynamicPlateauTracker<C, V> {
    fn on_observe(&mut self, gnodes: &Arena<GNode<C, V>>, g_id: GNodeId, value: V) {
        let mut cur = Some(g_id);
        while let Some(id) = cur {
            if let Some(key) = self.plateau_basis.plateau_key(id) {
                if let Some(p) = self.plateaus.get_mut(&key) {
                    p.sum = V::add(p.sum, value);
                }
            }
            cur = gnodes.get(id.index()).parent();
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
                            let g = gnodes.get(r.index());
                            (r.index(), format!("{:?}", g.sum()), format!("{:?}", g.state()))
                        })
                        .collect();
                    let expected: V = self
                        .plateau_basis
                        .basis_elements(&key)
                        .iter()
                        .map(|&r| gnodes.get(r.index()).sum())
                        .fold(V::zero(), V::add);
                    assert_eq!(
                        self.plateaus[&key].sum,
                        expected,
                        "POST-OBSERVE: plateau sum drift at key {key:?}\n\
                         delta={:?}, g_id=G({}), cur_node=G({})\n\
                         basis_elements={elems:?}",
                        value,
                        g_id.index(),
                        id.index(),
                    );
                }
                cur = gnodes.get(id.index()).parent();
            }
        }
    }

    fn on_bootstrap_split(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        g_id: GNodeId,
        _left_id: GNodeId,
    ) {
        let _span = tracing::debug_span!(
            "plateau_after_bootstrap_split",
            g_id = g_id.index(),
        )
        .entered();

        self.plateaus_dirty = true;

        let old_key = self
            .plateau_basis
            .remove(g_id)
            .expect("bootstrap_split: g_id must be a basis element");
        self.fixup_plateau(gnodes, old_key);

        let right_id = gnodes
            .get(g_id.index())
            .right()
            .expect("bootstrap_split: g_id must have a right child");
        let left_id = gnodes
            .get(g_id.index())
            .left()
            .expect("bootstrap_split: g_id must have a left child");

        let left_depth =
            gnode_depth_from_interval(gnodes.get(left_id.index()).lo(), gnodes.get(left_id.index()).hi(), self.n_bits);
        let right_depth =
            gnode_depth_from_interval(gnodes.get(right_id.index()).lo(), gnodes.get(right_id.index()).hi(), self.n_bits);

        if left_depth == right_depth {
            self.place_basis_element(gnodes, g_id, left_depth);
        } else {
            tracing::debug!(
                g = g_id.index(),
                left = left_id.index(),
                right = right_id.index(),
                left_depth,
                right_depth,
                "bootstrap_split: unequal child depths, placing children separately"
            );
            self.place_basis_element(gnodes, left_id, left_depth);
            self.place_basis_element(gnodes, right_id, right_depth);
        }
    }

    fn on_catalytic_split(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        g_id: GNodeId,
        left_id: GNodeId,
    ) {
        let _span = tracing::debug_span!(
            "plateau_after_catalytic_split",
            g_id = g_id.index(),
            left = left_id.index(),
        )
        .entered();

        self.plateaus_dirty = true;

        let right_id = gnodes
            .get(g_id.index())
            .right()
            .expect("catalytic_split: g_id must have a right child");

        let left_depth =
            gnode_depth_from_interval(gnodes.get(left_id.index()).lo(), gnodes.get(left_id.index()).hi(), self.n_bits);
        let right_depth =
            gnode_depth_from_interval(gnodes.get(right_id.index()).lo(), gnodes.get(right_id.index()).hi(), self.n_bits);

        // ── Phase 1: Locate the covering basis element for g_id ─────────────────
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
            let mut parent_opt = gnodes.get(g_id.index()).parent();
            let mut result = None;

            while let Some(p_id) = parent_opt {
                if let Some(key) = self.plateau_basis.remove(p_id) {
                    let mut displaced: Vec<GNodeId> = Vec::new();
                    for &path_node in &path {
                        let par = gnodes
                            .get(path_node.index())
                            .parent()
                            .expect("path node must have a parent");
                        let pg = gnodes.get(par.index());
                        let sibling = if pg.left() == Some(path_node) {
                            pg.right()
                        } else {
                            pg.left()
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
                parent_opt = gnodes.get(p_id.index()).parent();
            }

            result.expect("catalytic_split: no basis element covers g_id")
        };

        // ── Phase 2: Fixup the vacated plateau key ───────────────────────────
        self.fixup_plateau(gnodes, old_key);

        // ── Phase 3: Collect displaced elements and re-place ───────────────────
        let mut to_place = Vec::new();
        for &sib_id in &displaced {
            self.collect_subtree_basis_elements(gnodes, sib_id, &mut to_place);
        }
        if left_depth == right_depth {
            to_place.push((g_id, left_depth));
        } else {
            to_place.push((left_id, left_depth));
            to_place.push((right_id, right_depth));
        }
        self.place_sorted(gnodes, &mut to_place);
    }

    fn on_evict(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        gnode_id: GNodeId,
        parent_id: GNodeId,
        parent_state_after: GState,
        parent_lo: C,
        parent_hi: C,
    ) {
        let _span = tracing::debug_span!(
            "plateau_after_evict",
            gnode = gnode_id.index(),
            parent = parent_id.index(),
            ?parent_state_after,
        )
        .entered();

        // ── Phase 1: Remove evicted node and parent from plateau basis ────────────
        let evicted_key = self.plateau_basis.remove(gnode_id);

        let mut displaced: Vec<GNodeId> = Vec::new();
        let mut displaced_extra: Vec<(GNodeId, u32)> = Vec::new();

        let ancestor_key = if let Some(key) = self.plateau_basis.remove(parent_id) {
            if parent_state_after == GState::SemiInternal {
                let g = gnodes.get(parent_id.index());
                if let Some(sib) = g.left().or_else(|| g.right()) {
                    if let Some(ok) = self.plateau_basis.remove(sib) {
                        self.fixup_plateau(gnodes, ok);
                    }
                    displaced.push(sib);
                }
            }
            Some(key)
        } else {
            let mut path: Vec<GNodeId> = vec![parent_id];
            let mut cur = gnodes.get(parent_id.index()).parent();
            let mut found = None;

            let parent_key = {
                let pg = gnodes.get(parent_id.index());
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
                            let g = gnodes.get(parent_id.index());
                            if let Some(sib) = g.left().or_else(|| g.right()) {
                                if let Some(ok) = self.plateau_basis.remove(sib) {
                                    self.fixup_plateau(gnodes, ok);
                                }
                                displaced.push(sib);
                            }
                        }

                        for &path_node in &path {
                            let par = gnodes
                                .get(path_node.index())
                                .parent()
                                .expect("path node must have a parent");
                            let pg = gnodes.get(par.index());
                            let sibling = if pg.left() == Some(path_node) {
                                pg.right()
                            } else {
                                pg.left()
                            };
                            if let Some(sib_id) = sibling {
                                if displaced.contains(&sib_id) {
                                    continue;
                                }
                                if let Some(ok) = self.plateau_basis.remove(sib_id) {
                                    self.fixup_plateau(gnodes, ok);
                                }
                                displaced.push(sib_id);
                            }
                        }

                        found = Some(anc_key);
                        break;
                    }
                }
                path.push(anc);
                cur = gnodes.get(anc.index()).parent();
            }

            found
        };

        // ── Phase 2: Fixup displaced plateau keys ────────────────────────────────
        if let Some(ek) = evicted_key {
            self.fixup_plateau(gnodes, ek);
        }
        if let Some(ak) = ancestor_key {
            if evicted_key != Some(ak) {
                self.fixup_plateau(gnodes, ak);
            }
        }

        // ── Phase 3: Compute parent depth after eviction ─────────────────────────
        let parent_depth = match parent_state_after {
            GState::Terminal | GState::SemiInternal => {
                gnode_depth_from_interval(parent_lo, parent_hi, self.n_bits)
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
                .take_while(|(_, p)| {
                    p.start.total_cmp(&parent_hi) != std::cmp::Ordering::Greater
                })
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
                        self.collect_subtree_basis_elements(gnodes, m, &mut displaced_extra);
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
                        self.collect_subtree_basis_elements(gnodes, m, &mut displaced_extra);
                    }
                }
            }
        }

        // ── Phase 6: Collect displaced nodes and re-place ────────────────────────
        let mut to_place = Vec::new();
        to_place.push((parent_id, parent_depth));
        for &sib_id in &displaced {
            self.collect_subtree_basis_elements(gnodes, sib_id, &mut to_place);
        }
        to_place.extend(displaced_extra);
        self.place_sorted(gnodes, &mut to_place);
    }

    fn on_legacy_promotes_batched(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        new_gnodes: &[GNodeId],
    ) {
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
            let ng = gnodes.get(new_gid.index());
            let parent_id = ng
                .parent()
                .expect("legacy_promote child must have a parent");
            let child_depth = gnode_depth_from_interval(ng.lo(), ng.hi(), self.n_bits);

            let pg = gnodes.get(parent_id.index());
            let existing_child_id = if pg.left() == Some(new_gid) {
                pg.right()
            } else {
                pg.left()
            };

            if let Some(old_key) = self.plateau_basis.remove(parent_id) {
                self.fixup_plateau(gnodes, old_key);
            }

            if let Some(ec_id) = existing_child_id {
                let ec = gnodes.get(ec_id.index());
                if ec.state() != GState::Internal
                    && self.plateau_basis.plateau_key(ec_id).is_none()
                {
                    let existing_depth =
                        gnode_depth_from_interval(ec.lo(), ec.hi(), self.n_bits);
                    to_place.push((ec_id, existing_depth));
                }
            }
            to_place.push((new_gid, child_depth));
        }

        self.place_sorted(gnodes, &mut to_place);
    }

    #[allow(clippy::too_many_lines, clippy::float_cmp)]
    fn normalize(&mut self, gnodes: &Arena<GNode<C, V>>) {
        use crate::spatial::plateau::{BasisEdge, Plateau, basis_edge_of};

        if !self.plateaus_dirty {
            return;
        }
        self.plateaus_dirty = false;

        let _span = tracing::debug_span!("normalize_plateaus").entered();

        #[cfg(debug_assertions)]
        let old_total: V = self.plateaus.values().map(|p| p.sum).fold(V::zero(), V::add);

        // ── Phase 1: Collect and expand basis elements (DFS per basis node) ───────
        let mut elems: Vec<(GNodeId, BasisEdge<C>, u32, C, C, V)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let basis_ids: Vec<GNodeId> = self.plateau_basis.back_map().keys().copied().collect();
        for gid in basis_ids {
            let mut stack = vec![gid];
            while let Some(nid) = stack.pop() {
                if !seen.insert(nid) {
                    continue;
                }
                let g = gnodes.get(nid.index());
                match g.state() {
                    GState::Terminal => {
                        let depth = gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits);
                        elems.push((nid, basis_edge_of(g), depth, g.lo(), g.hi(), g.sum()));
                    }
                    GState::SemiInternal => {
                        let depth = gnode_depth_from_interval(g.lo(), g.hi(), self.n_bits);
                        elems.push((nid, basis_edge_of(g), depth, g.lo(), g.hi(), g.sum()));

                        if let Some(left) = g.left() {
                            stack.push(left);
                        }
                        if let Some(right) = g.right() {
                            stack.push(right);
                        }
                    }
                    GState::Internal => {
                        if let Some(ud) =
                            uniform_contour_depth_of(gnodes, nid, self.n_bits)
                        {
                            elems.push((nid, basis_edge_of(g), ud, g.lo(), g.hi(), g.sum()));
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
        }
        // ── Phase 2: Sort by basis-edge key ─────────────────────────────────────
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

        // ── Phase 3: Sweep-merge adjacent same-depth tiles ───────────────────────
        let mut new_plateaus: BTreeMap<BasisEdge<C>, Plateau<C, V>> = BTreeMap::new();
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
            let mut key_sums: BTreeMap<BasisEdge<C>, V> = BTreeMap::new();
            for (i, (key, _gid)) in assignments.iter().enumerate() {
                let entry = key_sums.entry(*key).or_insert(V::zero());
                *entry = V::add(*entry, elems[i].5);
            }
            for (key, &elem_sum) in &key_sums {
                let sweep_sum = new_plateaus[key].sum;
                debug_assert_eq!(
                    sweep_sum,
                    elem_sum,
                    "normalize_plateaus step 2: sweep sum mismatch for plateau {key:?}",
                );
            }
        }

        // ── Phase 4: Install new plateau map, rebuild basis, and consolidate ──────
        self.plateaus = new_plateaus;
        self.plateau_basis.rebuild(assignments);

        self.consolidate_all_basis(gnodes);

        #[cfg(debug_assertions)]
        {
            let new_total: V = self.plateaus.values().map(|p| p.sum).fold(V::zero(), V::add);
            debug_assert_eq!(
                old_total,
                new_total,
                "normalize_plateaus: total plateau energy changed after consolidation",
            );
        }
    }

    fn repair_p_i4(&mut self, gnodes: &Arena<GNode<C, V>>) {
        let span = tracing::debug_span!(
            "repair_p_i4",
            pending = self.pending_p_i4.len(),
            candidates = tracing::field::Empty,
        )
        .entered();

        let mut candidates: Vec<(GNodeId, BasisEdge<C>)> = Vec::new();
        for (&key, elements) in self.plateau_basis.iter() {
            for &gid in elements {
                if gnodes.is_occupied(gid.index())
                    && gnodes.get(gid.index()).state() == GState::SemiInternal
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
            if !gnodes.is_occupied(gid.index()) {
                continue;
            }
            let g = gnodes.get(gid.index());
            if g.state() != GState::SemiInternal {
                continue;
            }

            let Some(child_id) = g.left().or_else(|| g.right()) else {
                continue;
            };
            let child_lo = gnodes.get(child_id.index()).lo();
            let child_pk = self
                .plateaus
                .range(..=BasisEdge(child_lo))
                .next_back()
                .map(|(&k, _)| k);

            if child_pk == Some(pk) {
                self.split_for_p_i4(gnodes, pk, child_id);
            }
        }
    }

    fn recompute_sums(&mut self, gnodes: &Arena<GNode<C, V>>, label: &str) {
        for (&key, plateau) in &mut self.plateaus {
            plateau.sum = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&r| gnodes.get(r.index()).sum())
                .fold(V::zero(), V::add);
        }
        #[cfg(debug_assertions)]
        for (&key, plateau) in &self.plateaus {
            let expected: V = self
                .plateau_basis
                .basis_elements(&key)
                .iter()
                .map(|&r| gnodes.get(r.index()).sum())
                .fold(V::zero(), V::add);
            assert_eq!(
                plateau.sum, expected,
                "POST-{label}: plateau sum drift at key {key:?}"
            );
        }
    }

    fn set_dirty(&mut self) {
        self.plateaus_dirty = true;
    }

    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        Cow::Borrowed(&self.plateaus)
    }
}
