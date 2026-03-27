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

// ── Free functions (kept for callers that have not been migrated) ─────────────

/// Walks down the G-tree and returns the receiving terminal/semi-internal node.
#[must_use]
pub fn route_to_receiver<C: Coordinate, V: Accumulator>(
    gnodes: &Arena<GNode<C, V>>,
    root: GNodeId,
    x: C,
) -> GNodeId {
    let mut current = root;
    loop {
        let g = gnodes.get(current.index());
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

pub fn recompute_g_sums<C: Coordinate, V: Accumulator>(
    gnodes: &mut Arena<GNode<C, V>>,
    start: GNodeId,
) {
    let mut current = Some(start);
    while let Some(id) = current {
        let (left_sum, right_sum) = {
            let g = gnodes.get(id.index());
            let l = g.left().map_or_else(V::zero, |l| gnodes.get(l.index()).sum());
            let r = g.right().map_or_else(V::zero, |r| gnodes.get(r.index()).sum());
            (l, r)
        };
        let g = gnodes.get_mut(id.index());
        g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
        current = g.parent();
    }
}

pub fn recompute_g_sums_subtree<C: Coordinate, V: Accumulator>(
    gnodes: &mut Arena<GNode<C, V>>,
    preorder: &[GNodeId],
) {
    for &gid in preorder.iter().rev() {
        let (left_sum, right_sum) = {
            let g = gnodes.get(gid.index());
            let l = g.left().map_or_else(V::zero, |l| gnodes.get(l.index()).sum());
            let r = g.right().map_or_else(V::zero, |r| gnodes.get(r.index()).sum());
            (l, r)
        };
        let g = gnodes.get_mut(gid.index());
        g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
    }
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
