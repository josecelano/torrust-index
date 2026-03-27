//! V-tree — intensity-aggregation tree overlaid on the G-tree.
//!
//! The V-tree is a binary tree whose leaves (`VKind::Entry`) correspond
//! one-to-one with live G-node entry points.  Internal nodes
//! (`VKind::Structural`) aggregate the intensities of their children so that
//! any ancestor query can be answered in O(depth) time.
//!
//! ## Node kinds
//!
//! - **`VKind::Entry`**: a leaf that stores the intensity contributed by one
//!   G-node.  It also carries the `GNodeId` it belongs to, enabling the
//!   G-tree to look up its V-node in O(1).
//! - **`VKind::Structural`**: an internal node whose `intensity` is always
//!   the sum of all descendant entry intensities.  Its `children` array holds
//!   up to 3 entries (2 in the balanced case; 3 is transient and triggers a
//!   rebalance violation).
//!
//! ## Update patterns
//!
//! Three contexts require different amounts of work:
//!
//! 1. **Point update + ancestor propagate** (`observe`): after a single
//!    G-node's own value changes, call [`sync_intensity_in_parent`] to update
//!    the entry's cached slot in its parent, then [`propagate_v_sums`] to walk
//!    up to the root recomputing structural intensities.  O(depth) work.
//!
//! 2. **Full post-order recompute** (`decay`): after a bulk operation that
//!    changes many G-nodes at once, call [`recompute_all_v_intensities`] which
//!    visits every V-node in post-order.  O(n) work, but correct regardless of
//!    which entries changed.
//!
//! 3. **Depth invalidation** (`split`, `evict`): structural changes (inserts,
//!    removes) invalidate cached depth values.  Call
//!    [`invalidate_depth_subtree`] to mark a subtree stale; depths are
//!    recomputed on demand via [`v_depth`].
//!
//! ## Rebalance violations
//!
//! A V-node is *violated* when its intensity distribution across children
//! breaches the configured balance threshold.  See `rebalance.rs` for the
//! `is_violated` predicate and the `resolve` function that repairs violations.
use crate::arena::Arena;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{Children, DEPTH_STALE, VKind, VNode};
use crate::traits::{Accumulator, Coordinate};

// ── VTree ────────────────────────────────────────────────────────────────────

/// The V-tree: an intensity-aggregation binary tree overlaid on the G-tree.
/// Owns the node arena, the root pointer, and the violations queue.
#[derive(Debug, Clone)]
pub struct VTree<V: Accumulator> {
    /// Backing store for all V-nodes.
    pub(crate) nodes: Arena<VNode<V>>,
    /// Root V-node (`None` only when the tree is empty).
    pub(crate) root: Option<VNodeId>,
    /// V-nodes whose intensity distribution violates the balance threshold.
    pub(crate) violations: Vec<VNodeId>,
}

// ── VTree methods ─────────────────────────────────────────────────────────────

impl<V: Accumulator> VTree<V> {
    // ── Structural modifications ──────────────────────────────────────────

    /// Removes leaf `v_id` from the tree and updates `self.root` in place.
    pub(crate) fn remove_leaf<C: Coordinate>(
        &mut self,
        gnodes: &mut Arena<GNode<C, V>>,
        v_id: VNodeId,
    ) {
        self.root = vtree_remove_leaf(&mut self.nodes, gnodes, v_id, self.root);
    }

    // ── Sum / intensity propagation ───────────────────────────────────────

    pub(crate) fn propagate_sums(&mut self, id: VNodeId) {
        propagate_v_sums(&mut self.nodes, id);
    }

    pub(crate) fn sync_intensity(&mut self, id: VNodeId, val: V) {
        sync_intensity_in_parent(&mut self.nodes, id, val);
    }

    pub(crate) fn recompute_all_intensities(&mut self) {
        if let Some(root) = self.root {
            recompute_all_v_intensities(&mut self.nodes, root);
        }
    }

    // ── Depth cache ───────────────────────────────────────────────────────

    pub(crate) fn depth(&self, id: VNodeId) -> u32 {
        v_depth(&self.nodes, id)
    }

    // ── Evictable flags ───────────────────────────────────────────────────

    pub(crate) fn propagate_evictable(&mut self, id: VNodeId) {
        propagate_evictable_flags(&mut self.nodes, id);
    }

    // ── Eviction candidate scan ───────────────────────────────────────────

    /// Returns all V-entry nodes eligible for eviction.
    ///
    /// A node is a candidate when it is deeper than `live_depth_evict`,
    /// flagged `is_evictable`, and does not belong to the G-tree root.
    pub(crate) fn scan_for_candidates(
        &self,
        live_depth_evict: u32,
        g_root: GNodeId,
    ) -> Vec<VNodeId> {
        let _span = tracing::trace_span!("scan_for_candidates").entered();
        let mut candidates = Vec::new();
        if let Some(v_root) = self.root {
            self.scan_dfs(v_root, 0, live_depth_evict, g_root, &mut candidates);
        }
        tracing::trace!(candidates = candidates.len(), "scan complete");
        candidates
    }

    fn scan_dfs(
        &self,
        v_id: VNodeId,
        depth: u32,
        live_depth_evict: u32,
        g_root: GNodeId,
        candidates: &mut Vec<VNodeId>,
    ) {
        let node = self.nodes.get(v_id.index());
        match &node.kind() {
            VKind::Entry {
                gnode,
                is_evictable,
                ..
            } => {
                if depth > live_depth_evict && *is_evictable && *gnode != g_root {
                    candidates.push(v_id);
                }
            }
            VKind::Structural {
                children,
                has_evictable,
            } => {
                if !has_evictable {
                    return;
                }
                for i in 0..children.len() {
                    let (child_id, _) = children.get(i);
                    self.scan_dfs(child_id, depth + 1, live_depth_evict, g_root, candidates);
                }
            }
        }
    }
}

pub fn vtree_remove_leaf<C: Coordinate, V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    gnodes: &mut Arena<GNode<C, V>>,
    v_id: VNodeId,
    v_root: Option<VNodeId>,
) -> Option<VNodeId> {
    let span = tracing::debug_span!(
        "vtree_remove_leaf",
        v_id = v_id.index(),
        case = tracing::field::Empty,
    )
    .entered();

    if let VKind::Entry { gnode, .. } = vnodes.get(v_id.index()).kind() {
        gnodes.get_mut(gnode.index()).clear_entry();
    }

    let parent = vnodes.get(v_id.index()).parent();

    let Some(p_id) = parent else {
        span.record("case", "root");
        vnodes.dealloc(v_id.index());
        return None;
    };

    let p = vnodes.get(p_id.index());
    let p_child_count = match &p.kind() {
        VKind::Structural { children, .. } => children.len(),
        VKind::Entry { .. } => unreachable!("parent of entry should be structural"),
    };

    if p_child_count == 3 {
        span.record("case", "shrink");
        remove_child_from_structural(vnodes, p_id, v_id);
        recompute_and_propagate_v_sums(vnodes, p_id);
        propagate_evictable_flags(vnodes, p_id);
        vnodes.dealloc(v_id.index());
        return v_root;
    }

    span.record("case", "collapse");
    let sole_id = sole_sibling(vnodes, p_id, v_id);
    let grandparent = vnodes.get(p_id.index()).parent();

    vnodes.get_mut(sole_id.index()).set_parent_opt(grandparent);

    invalidate_depth_subtree(vnodes, sole_id);

    let new_root = grandparent.map_or(Some(sole_id), |g_id| {
        let sole_int = vnodes.get(sole_id.index()).intensity();
        replace_child_in_parent(vnodes, g_id, p_id, sole_id, sole_int);
        recompute_and_propagate_v_sums(vnodes, g_id);
        propagate_evictable_flags(vnodes, g_id);
        v_root
    });

    vnodes.dealloc(p_id.index());
    vnodes.dealloc(v_id.index());
    new_root
}

/// Walks ancestors of `start` (exclusive — `start` itself is not recomputed)
/// and updates each structural node's intensity and its cached slot in its parent.
/// Use [`recompute_and_propagate_v_sums`] when `start` also needs recomputing.
pub fn propagate_v_sums<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    tracing::trace!(start = start.index(), "propagate_v_sums");
    let mut current = vnodes.get(start.index()).parent();
    while let Some(id) = current {
        recompute_structural_intensity(vnodes, id);
        let new_int = vnodes.get(id.index()).intensity();
        sync_intensity_in_parent(vnodes, id, new_int);
        current = vnodes.get(id.index()).parent();
    }
}

/// Recomputes intensities for every node in the tree rooted at `v_root`
/// (full post-order traversal). Use [`propagate_v_sums`] for a cheaper
/// ancestor-only walk after a targeted update.
pub fn recompute_all_v_intensities<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, v_root: VNodeId) {
    recompute_v_postorder(vnodes, v_root);
}

fn recompute_v_postorder<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, id: VNodeId) {
    // Collect child IDs while holding a shared borrow, then release it so the
    // recursive calls and the subsequent mutable borrows can proceed.
    let child_ids: Vec<VNodeId> = {
        let node = vnodes.get(id.index());
        match &node.kind() {
            VKind::Entry { .. } => return,
            VKind::Structural { children, .. } => {
                (0..children.len()).map(|i| children.get(i).0).collect()
            }
        }
    };

    for &child in &child_ids {
        recompute_v_postorder(vnodes, child);
    }

    for (i, &child) in child_ids.iter().enumerate() {
        let child_int = vnodes.get(child.index()).intensity();
        let node = vnodes.get_mut(id.index());
        if let VKind::Structural { children, .. } = node.kind_mut() {
            children.update_intensity(i, child_int);
        }
    }

    // Sum updated child intensities. The shared borrow must end (NLL last-use)
    // before the mutable borrow on the next line, so total is computed first.
    let total: V = {
        let node = vnodes.get(id.index());
        let VKind::Structural { children, .. } = &node.kind() else {
            return;
        };
        let mut t = V::zero();
        for i in 0..children.len() {
            t = V::add(t, children.get(i).1);
        }
        t
    };
    vnodes.get_mut(id.index()).set_intensity(total);
}

pub fn propagate_evictable_flags<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    let mut current = Some(start);
    while let Some(id) = current {
        let node = vnodes.get(id.index());
        match &node.kind() {
            VKind::Entry { .. } => {
                current = node.parent();
            }
            VKind::Structural {
                children,
                has_evictable,
            } => {
                let old = *has_evictable;
                let new_flag = compute_has_evictable(vnodes, children);
                if new_flag == old {
                    return;
                }

                let parent = node.parent();
                set_has_evictable(vnodes, id, new_flag);
                current = parent;
            }
        }
    }
}

#[cfg(debug_assertions)]
fn v_depth_uncached<V: Accumulator>(vnodes: &Arena<VNode<V>>, id: VNodeId) -> u32 {
    let node = vnodes.get(id.index());
    node.parent().map_or(0, |p| v_depth_uncached(vnodes, p) + 1)
}

#[must_use]
pub fn v_depth<V: Accumulator>(vnodes: &Arena<VNode<V>>, id: VNodeId) -> u32 {
    let node = vnodes.get(id.index());
    let cached = node.cached_depth_raw();

    if cached != DEPTH_STALE {
        #[cfg(debug_assertions)]
        assert_eq!(
            cached,
            v_depth_uncached(vnodes, id),
            "cached depth mismatch for VNode {id:?}"
        );
        return cached;
    }

    let depth = node.parent().map_or(0, |p| v_depth(vnodes, p) + 1);
    node.store_depth(depth);
    depth
}

pub fn invalidate_depth_subtree<V: Accumulator>(vnodes: &Arena<VNode<V>>, root: VNodeId) {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = vnodes.get(id.index());

        if node.cached_depth_raw() == DEPTH_STALE {
            continue;
        }

        node.store_depth(DEPTH_STALE);

        if let VKind::Structural { children, .. } = &node.kind() {
            for i in 0..children.len() {
                stack.push(children.get(i).0);
            }
        }
    }
}

pub fn sync_intensity_in_parent<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    child_id: VNodeId,
    new_intensity: V,
) {
    let parent = vnodes.get(child_id.index()).parent();
    let Some(p_id) = parent else { return };
    let p = vnodes.get_mut(p_id.index());
    if let VKind::Structural { children, .. } = p.kind_mut() {
        if let Some(idx) = children.find_index(child_id) {
            children.update_intensity(idx, new_intensity);
        }
    }
}

pub fn replace_child_in_parent<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    parent: VNodeId,
    old_child: VNodeId,
    new_child: VNodeId,
    new_intensity: V,
) {
    let p = vnodes.get_mut(parent.index());
    if let VKind::Structural { children, .. } = p.kind_mut() {
        children.replace_child(old_child, new_child, new_intensity);
    }
}

fn remove_child_from_structural<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    parent: VNodeId,
    child: VNodeId,
) {
    let p = vnodes.get_mut(parent.index());
    if let VKind::Structural { children, .. } = p.kind_mut() {
        children.remove_child(child);
    }
}

fn sole_sibling<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    parent: VNodeId,
    child: VNodeId,
) -> VNodeId {
    let p = vnodes.get(parent.index());
    if let VKind::Structural { children, .. } = &p.kind() {
        for i in 0..children.len() {
            let (id, _) = children.get(i);
            if id != child {
                return id;
            }
        }
    }
    unreachable!("sole_sibling: child not found in parent");
}

pub fn recompute_structural_intensity<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, id: VNodeId) {
    let node = vnodes.get(id.index());
    if let VKind::Structural { children, .. } = &node.kind() {
        let mut total = V::zero();
        for i in 0..children.len() {
            total = V::add(total, children.get(i).1);
        }

        let _ = node;
        vnodes.get_mut(id.index()).set_intensity(total);
    }
}

fn recompute_and_propagate_v_sums<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    recompute_structural_intensity(vnodes, start);
    let new_int = vnodes.get(start.index()).intensity();
    sync_intensity_in_parent(vnodes, start, new_int);
    propagate_v_sums(vnodes, start);
}

fn compute_has_evictable<V: Accumulator>(vnodes: &Arena<VNode<V>>, children: &Children<V>) -> bool {
    for i in 0..children.len() {
        let (child_id, _) = children.get(i);
        let child = vnodes.get(child_id.index());
        let child_flag = match &child.kind() {
            VKind::Entry { is_evictable, .. } => *is_evictable,
            VKind::Structural { has_evictable, .. } => *has_evictable,
        };
        if child_flag {
            return true;
        }
    }
    false
}

fn set_has_evictable<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, id: VNodeId, flag: bool) {
    let node = vnodes.get_mut(id.index());
    if let VKind::Structural { has_evictable, .. } = node.kind_mut() {
        *has_evictable = flag;
    }
}

/// Returns `true` if `ancestor` is a strict ancestor of `descendant` in the V-tree
/// (i.e. reachable by following parent links from `descendant`).
pub fn is_ancestor<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    ancestor: VNodeId,
    mut descendant: VNodeId,
) -> bool {
    while let Some(p) = vnodes.get(descendant.index()).parent() {
        if p == ancestor {
            return true;
        }
        descendant = p;
    }
    false
}

#[cfg(test)]
mod tests {

    use super::{invalidate_depth_subtree, propagate_v_sums, v_depth, vtree_remove_leaf};
    use crate::arena::Arena;
    use crate::handle::{GNodeId, VNodeId};
    use crate::nodes::vnode::{Children, DEPTH_STALE, VKind, VNode};

    fn entry_vnode(intensity: u32, parent: Option<VNodeId>) -> VNode<u32> {
        VNode::new_entry(
            intensity,
            parent,
            DEPTH_STALE,
            GNodeId::from_index(0),
            true,
            true,
        )
    }

    // ── v_depth ──────────────────────────────────────────────────────────
    mod v_depth_fn {
        use super::*;

        #[test]
        fn root_node_has_depth_zero() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            assert_eq!(v_depth(&vnodes, id), 0);
        }

        #[test]
        fn child_of_root_has_depth_one() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let root_id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            let child_id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, Some(root_id))));
            assert_eq!(v_depth(&vnodes, child_id), 1);
        }

        #[test]
        fn result_is_cached_after_first_call() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            let d1 = v_depth(&vnodes, id);
            let d2 = v_depth(&vnodes, id);
            assert_eq!(d1, d2);
            // Verify the cached value is no longer DEPTH_STALE
            let cached = vnodes.get(id.index()).cached_depth_raw();
            assert_ne!(cached, DEPTH_STALE);
        }
    }

    // ── invalidate_depth_subtree ─────────────────────────────────────────
    mod invalidate_depth_subtree_fn {
        use super::*;

        #[test]
        fn marks_cached_depth_stale_on_root() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            // Pre-populate the cached_depth to a non-stale value
            let node = entry_vnode(0, None);
            node.store_depth(0);
            let id = VNodeId::from_index(vnodes.alloc(node));

            invalidate_depth_subtree(&vnodes, id);

            let cached = vnodes.get(id.index()).cached_depth_raw();
            assert_eq!(cached, DEPTH_STALE);
        }

        #[test]
        fn leaves_already_stale_nodes_unchanged() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            // cached_depth is already DEPTH_STALE from entry_vnode helper
            let id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            invalidate_depth_subtree(&vnodes, id); // should not panic
            let cached = vnodes.get(id.index()).cached_depth_raw();
            assert_eq!(cached, DEPTH_STALE);
        }
    }

    // ── propagate_v_sums ─────────────────────────────────────────────────
    mod propagate_v_sums_fn {
        use super::*;

        #[test]
        fn propagating_from_root_entry_does_not_panic() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let root_id = VNodeId::from_index(vnodes.alloc(entry_vnode(10, None)));
            // Root has no parent; propagate_v_sums is a no-op but must not panic
            propagate_v_sums(&mut vnodes, root_id);
        }

        #[test]
        fn propagating_from_child_updates_structural_parent() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();

            // Allocate two placeholder slots to get stable IDs
            let child_a_id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            let child_b_id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));

            let parent_node: VNode<u32> = VNode::new_structural(
                0,
                None,
                DEPTH_STALE,
                Children::new_2((child_a_id, 5u32), (child_b_id, 7u32)),
                false,
            );
            let parent_id = VNodeId::from_index(vnodes.alloc(parent_node));

            // Wire children back to parent
            vnodes.get_mut(child_a_id.index()).set_parent(parent_id);
            vnodes.get_mut(child_b_id.index()).set_parent(parent_id);

            // Update child_a's own intensity
            vnodes.get_mut(child_a_id.index()).set_intensity(20);

            propagate_v_sums(&mut vnodes, child_a_id);

            // Parent intensity should now reflect sum of cached child intensities
            // (The structural node caches 5 and 7; propagate_v_sums recomputes from them)
            let parent_intensity = vnodes.get(parent_id.index()).intensity();
            assert_eq!(parent_intensity, 5 + 7); // cached intensities in Children
        }
    }

    // ── vtree_remove_leaf ─────────────────────────────────────────────────
    mod vtree_remove_leaf_fn {
        use super::*;
        use crate::nodes::gnode::GNode;

        /// Minimal gnodes arena with one Terminal GNode at index 0.
        fn gnodes_with_one_node() -> Arena<GNode<u8, u32>> {
            let mut gnodes: Arena<GNode<u8, u32>> = Arena::new();
            gnodes.alloc(GNode::new_leaf(0u8, 255u8, 0u32, None));
            gnodes
        }

        // Root-removal: node has no parent → returns None.
        #[test]
        fn root_removal_returns_none() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let mut gnodes = gnodes_with_one_node();
            let v_root = VNodeId::from_index(vnodes.alloc(entry_vnode(10, None)));
            let result = vtree_remove_leaf(&mut vnodes, &mut gnodes, v_root, Some(v_root));
            assert!(result.is_none());
            assert!(!vnodes.is_occupied(v_root.index()));
        }

        // Shrink case: parent has 3 children → remove one, parent shrinks to 2.
        #[test]
        fn shrink_removes_child_from_three_child_parent() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let mut gnodes = gnodes_with_one_node();

            let child_a = VNodeId::from_index(vnodes.alloc(entry_vnode(5, None)));
            let child_b = VNodeId::from_index(vnodes.alloc(entry_vnode(5, None)));
            let child_c = VNodeId::from_index(vnodes.alloc(entry_vnode(5, None)));

            let parent_id = VNodeId::from_index(vnodes.alloc(VNode::new_structural(
                15,
                None,
                DEPTH_STALE,
                Children::new_3((child_a, 5u32), (child_b, 5u32), (child_c, 5u32)),
                true,
            )));

            vnodes.get_mut(child_a.index()).set_parent(parent_id);
            vnodes.get_mut(child_b.index()).set_parent(parent_id);
            vnodes.get_mut(child_c.index()).set_parent(parent_id);

            let result = vtree_remove_leaf(&mut vnodes, &mut gnodes, child_c, Some(parent_id));
            assert_eq!(result, Some(parent_id)); // parent remains root
            assert!(!vnodes.is_occupied(child_c.index())); // target removed
            match &vnodes.get(parent_id.index()).kind() {
                VKind::Structural { children, .. } => assert_eq!(children.len(), 2),
                _ => panic!("expected Structural"),
            }
        }

        // Collapse / no-grandparent: 2-child parent is root; sole sibling becomes new root.
        #[test]
        fn collapse_no_grandparent_sole_sibling_becomes_root() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            let mut gnodes = gnodes_with_one_node();

            let target = VNodeId::from_index(vnodes.alloc(entry_vnode(5, None)));
            let sibling = VNodeId::from_index(vnodes.alloc(entry_vnode(5, None)));

            let parent_id = VNodeId::from_index(vnodes.alloc(VNode::new_structural(
                10,
                None,
                DEPTH_STALE,
                Children::new_2((target, 5u32), (sibling, 5u32)),
                true,
            )));

            vnodes.get_mut(target.index()).set_parent(parent_id);
            vnodes.get_mut(sibling.index()).set_parent(parent_id);

            let result = vtree_remove_leaf(&mut vnodes, &mut gnodes, target, Some(parent_id));
            assert_eq!(result, Some(sibling)); // sibling is new root
            assert!(!vnodes.is_occupied(target.index()));
            assert!(!vnodes.is_occupied(parent_id.index())); // parent collapsed
            assert!(vnodes.get(sibling.index()).parent().is_none());
        }
    }
}
