use crate::arena::Arena;
use crate::handle::GNodeId;
use crate::nodes::gnode::GNode;
use crate::traits::{Accumulator, Coordinate};

// ── GTree ────────────────────────────────────────────────────────────────────

/// The G-tree: a binary spatial-partition tree whose leaves are the observable
/// coordinate ranges.  Owns the node arena plus the operational parameters
/// that govern growth and eviction.
#[derive(Debug, Clone)]
pub struct GTree<C: Coordinate, V: Accumulator, const N: u32> {
    /// Backing store for all G-nodes.
    pub(crate) nodes: Arena<GNode<C, V>>,
    /// Root G-node (always present).
    pub(crate) root: GNodeId,
    /// Total number of live G-nodes (terminals + internal).
    pub(crate) node_count: u32,
    /// Number of terminal (leaf) G-nodes.
    pub(crate) terminal_count: u32,
    /// Maximum depth at which live V-entries can exist before eviction.
    pub(crate) live_depth_evict: u32,
    /// Maximum depth at which new V-entries are created.
    pub(crate) live_depth_create: u32,
    /// `depth_evict - depth_create`.
    pub(crate) depth_buffer: u32,
    /// Maximum nodes in the `[depth_create, depth_evict]` band.
    pub(crate) headroom: usize,
    /// Soft node-count limit that triggers eviction (`budget - headroom`).
    pub(crate) soft_limit: Option<usize>,
}

impl<C: Coordinate, V: Accumulator, const N: u32> GTree<C, V, N> {
    // ── Traversal ────────────────────────────────────────────────────────

    /// Walks down the G-tree from the root and returns the terminal (or
    /// semi-internal) G-node that *receives* coordinate `x`.
    #[must_use]
    pub(crate) fn route_to(&self, x: C) -> GNodeId {
        let mut current = self.root;
        loop {
            let g = self.nodes.get(current.index());
            let mid = C::midpoint(g.lo(), g.hi());
            if x < mid {
                if let Some(left) = g.left() {
                    current = left;
                } else {
                    return current;
                }
            } else if let Some(right) = g.right() {
                current = right;
            } else {
                return current;
            }
        }
    }

    // ── Sum recomputation ────────────────────────────────────────────────

    /// Walks from `start` toward the root, recomputing `sum` at each node.
    pub(crate) fn recompute_sums(&mut self, start: GNodeId) {
        let mut current = Some(start);
        while let Some(id) = current {
            let (left_sum, right_sum) = {
                let g = self.nodes.get(id.index());
                let l = g
                    .left()
                    .map_or_else(V::zero, |l| self.nodes.get(l.index()).sum());
                let r = g
                    .right()
                    .map_or_else(V::zero, |r| self.nodes.get(r.index()).sum());
                (l, r)
            };
            let g = self.nodes.get_mut(id.index());
            g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
            current = g.parent();
        }
    }

    /// Recomputes `sum` for each node in `preorder` (processed in reverse,
    /// i.e. leaves-first).
    pub(crate) fn recompute_sums_subtree(&mut self, preorder: &[GNodeId]) {
        for &gid in preorder.iter().rev() {
            let (left_sum, right_sum) = {
                let g = self.nodes.get(gid.index());
                let l = g
                    .left()
                    .map_or_else(V::zero, |l| self.nodes.get(l.index()).sum());
                let r = g
                    .right()
                    .map_or_else(V::zero, |r| self.nodes.get(r.index()).sum());
                (l, r)
            };
            let g = self.nodes.get_mut(gid.index());
            g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
        }
    }

    // ── Depth helpers ────────────────────────────────────────────────────

    /// Returns the depth of a G-node whose interval is `[lo, hi)` in an
    /// `N`-bit domain.
    #[must_use]
    #[inline]
    pub(crate) fn depth_of_interval(lo: C, hi: C) -> u32 {
        gnode_depth_from_interval(lo, hi, N)
    }

    /// Returns the uniform contour depth of the subtree rooted at `gid`, or
    /// `None` if the leaf G-nodes in the subtree do not all share the same depth.
    #[cfg(feature = "dynamic-contour-tracking")]
    #[must_use]
    pub(crate) fn uniform_contour_depth(&self, gid: GNodeId) -> Option<u32> {
        uniform_contour_depth_of(&self.nodes, gid, N)
    }

    // ── G-node allocation / eviction helpers ─────────────────────────────

    /// Allocates two new leaf G-nodes that split `parent_id` at its midpoint,
    /// links them as left and right children of `parent_id`, and updates the
    /// node / terminal counts (`node_count += 2`, `terminal_count += 1`).
    /// Returns `(left_id, right_id)`.
    pub(crate) fn allocate_children(&mut self, parent_id: GNodeId) -> (GNodeId, GNodeId) {
        let (lo, hi) = {
            let g = self.nodes.get(parent_id.index());
            (g.lo(), g.hi())
        };
        let mid = C::midpoint(lo, hi);
        let left_id = GNodeId::from_index(self.nodes.alloc(GNode::new_leaf(
            lo,
            mid,
            V::zero(),
            Some(parent_id),
        )));
        let right_id = GNodeId::from_index(self.nodes.alloc(GNode::new_leaf(
            mid,
            hi,
            V::zero(),
            Some(parent_id),
        )));
        self.nodes.get_mut(parent_id.index()).link_left(left_id);
        self.nodes.get_mut(parent_id.index()).link_right(right_id);
        self.node_count += 2;
        // The parent was a terminal; it is now internal. Two new terminals are
        // added, the parent terminal is lost: net change = +2 − 1 = +1.
        self.terminal_count += 1;
        (left_id, right_id)
    }

    /// Absorbs `child_id`'s accumulated sum into its parent's own weight,
    /// clears the child link from the parent, and recomputes the parent's sum
    /// invariant.  Returns the parent `GNodeId`.
    ///
    /// **Note:** this method does *not* deallocate `child_id` — the caller is
    /// responsible for deallocation after any remaining cross-tree work that
    /// may still read the child slot (e.g. `vtree_remove_leaf`).
    pub(crate) fn merge_into_parent(&mut self, child_id: GNodeId) -> GNodeId {
        let parent_id = self
            .nodes
            .get(child_id.index())
            .parent()
            .expect("merge_into_parent: child must have a parent");

        let child_sum = self.nodes.get(child_id.index()).sum();
        let parent_own_before = self.nodes.get(parent_id.index()).own();
        self.nodes
            .get_mut(parent_id.index())
            .set_own(V::add(parent_own_before, child_sum));
        self.nodes.get_mut(parent_id.index()).clear_child(child_id);

        let new_sum = {
            let p = self.nodes.get(parent_id.index());
            let left_sum = p
                .left()
                .map_or_else(V::zero, |l| self.nodes.get(l.index()).sum());
            let right_sum = p
                .right()
                .map_or_else(V::zero, |r| self.nodes.get(r.index()).sum());
            V::add(p.own(), V::add(left_sum, right_sum))
        };
        self.nodes.get_mut(parent_id.index()).set_sum(new_sum);

        parent_id
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

#[must_use]
#[inline]
pub fn gnode_depth_from_interval<C: Coordinate>(lo: C, hi: C, n: u32) -> u32 {
    let width_f64 = C::width(lo, hi).to_f64();
    debug_assert!(
        width_f64 > 0.0,
        "gnode_depth_from_interval: zero-width interval"
    );

    #[allow(clippy::cast_possible_truncation)]
    let log2_width = width_f64.log2() as i32;
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let depth = (n as i32 - log2_width) as u32;
    depth
}

/// Returns the uniform contour depth of the subtree rooted at `gid`, or `None`
/// if the leaf G-nodes do not all share the same depth.
#[cfg(feature = "dynamic-contour-tracking")]
#[must_use]
pub fn uniform_contour_depth_of<C: Coordinate, V: Accumulator>(
    gnodes: &Arena<GNode<C, V>>,
    gid: GNodeId,
    n: u32,
) -> Option<u32> {
    use crate::nodes::gnode::GState;
    let g = gnodes.get(gid.index());
    match g.state() {
        GState::Terminal => Some(gnode_depth_from_interval(g.lo(), g.hi(), n)),
        GState::SemiInternal => None,
        GState::Internal => {
            let ld = g
                .left()
                .and_then(|l| uniform_contour_depth_of(gnodes, l, n))?;
            let rd = g
                .right()
                .and_then(|r| uniform_contour_depth_of(gnodes, r, n))?;
            if ld == rd { Some(ld) } else { None }
        }
    }
}
