use std::sync::atomic::Ordering;

use crate::arena::Arena;
use crate::handle::VNodeId;
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{DEPTH_STALE, PackedChildren, VKind, VNode};
use crate::traits::{Accumulator, Coordinate};

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

    if let VKind::Entry { gnode, .. } = vnodes.get(v_id.index()).kind {
        gnodes.get_mut(gnode.index()).entry = None;
    }

    let parent = vnodes.get(v_id.index()).parent;

    let Some(p_id) = parent else {
        span.record("case", "root");
        vnodes.dealloc(v_id.index());
        return None;
    };

    let p = vnodes.get(p_id.index());
    let p_child_count = match &p.kind {
        VKind::Structural { children, .. } => children.len(),
        VKind::Entry { .. } => unreachable!("parent of entry should be structural"),
    };

    if p_child_count == 3 {
        span.record("case", "shrink");
        remove_child_from_structural(vnodes, p_id, v_id);
        propagate_v_sums_from(vnodes, p_id);
        propagate_evictable_flags(vnodes, p_id);
        vnodes.dealloc(v_id.index());
        return v_root;
    }

    span.record("case", "collapse");
    let sole_id = sole_sibling(vnodes, p_id, v_id);
    let grandparent = vnodes.get(p_id.index()).parent;

    vnodes.get_mut(sole_id.index()).parent = grandparent;

    invalidate_depth_subtree(vnodes, sole_id);

    let new_root = grandparent.map_or(Some(sole_id), |g_id| {
        let sole_int = vnodes.get(sole_id.index()).intensity;
        replace_child_in_parent(vnodes, g_id, p_id, sole_id, sole_int);
        propagate_v_sums_from(vnodes, g_id);
        propagate_evictable_flags(vnodes, g_id);
        v_root
    });

    vnodes.dealloc(p_id.index());
    vnodes.dealloc(v_id.index());
    new_root
}

pub fn propagate_v_sums<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    tracing::trace!(start = start.index(), "propagate_v_sums");
    let mut current = vnodes.get(start.index()).parent;
    while let Some(id) = current {
        recompute_structural_intensity(vnodes, id);
        let new_int = vnodes.get(id.index()).intensity;
        update_parent_cached_intensity(vnodes, id, new_int);
        current = vnodes.get(id.index()).parent;
    }
}

pub fn recompute_all_v_intensities<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, v_root: VNodeId) {
    recompute_v_postorder(vnodes, v_root);
}

fn recompute_v_postorder<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, id: VNodeId) {
    let child_ids: Option<Vec<VNodeId>> = {
        let node = vnodes.get(id.index());
        match &node.kind {
            VKind::Entry { .. } => None,
            VKind::Structural { children, .. } => {
                let ids: Vec<VNodeId> = (0..children.len()).map(|i| children.get(i).0).collect();
                Some(ids)
            }
        }
    };

    let Some(child_ids) = child_ids else {
        return;
    };

    for &child in &child_ids {
        recompute_v_postorder(vnodes, child);
    }

    for (i, &child) in child_ids.iter().enumerate() {
        let child_int = vnodes.get(child.index()).intensity;
        let node = vnodes.get_mut(id.index());
        if let VKind::Structural { children, .. } = &mut node.kind {
            children.intensities[i] = child_int;
        }
    }

    let node = vnodes.get(id.index());
    if let VKind::Structural { children, .. } = &node.kind {
        let mut total = V::zero();
        for i in 0..children.len() {
            total = V::add(total, children.intensities[i]);
        }

        let _ = node;
        vnodes.get_mut(id.index()).intensity = total;
    }
}

pub fn propagate_evictable_flags<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    let mut current = Some(start);
    while let Some(id) = current {
        let node = vnodes.get(id.index());
        match &node.kind {
            VKind::Entry { .. } => {
                current = node.parent;
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

                let parent = node.parent;
                set_has_evictable(vnodes, id, new_flag);
                current = parent;
            }
        }
    }
}

#[cfg(debug_assertions)]
fn v_depth_uncached<V: Accumulator>(vnodes: &Arena<VNode<V>>, id: VNodeId) -> u32 {
    let node = vnodes.get(id.index());
    node.parent.map_or(0, |p| v_depth_uncached(vnodes, p) + 1)
}

#[must_use]
pub fn v_depth<V: Accumulator>(vnodes: &Arena<VNode<V>>, id: VNodeId) -> u32 {
    let node = vnodes.get(id.index());
    let cached = node.cached_depth.load(Ordering::Relaxed);

    if cached != DEPTH_STALE {
        #[cfg(debug_assertions)]
        assert_eq!(
            cached,
            v_depth_uncached(vnodes, id),
            "cached depth mismatch for VNode {id:?}"
        );
        return cached;
    }

    let depth = node.parent.map_or(0, |p| v_depth(vnodes, p) + 1);
    node.cached_depth.store(depth, Ordering::Relaxed);
    depth
}

pub fn invalidate_depth_subtree<V: Accumulator>(vnodes: &Arena<VNode<V>>, root: VNodeId) {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = vnodes.get(id.index());

        if node.cached_depth.load(Ordering::Relaxed) == DEPTH_STALE {
            continue;
        }

        node.cached_depth.store(DEPTH_STALE, Ordering::Relaxed);

        if let VKind::Structural { children, .. } = &node.kind {
            for i in 0..children.len() {
                stack.push(children.get(i).0);
            }
        }
    }
}

pub fn update_parent_cached_intensity<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    child_id: VNodeId,
    new_intensity: V,
) {
    let parent = vnodes.get(child_id.index()).parent;
    let Some(p_id) = parent else { return };
    let p = vnodes.get_mut(p_id.index());
    if let VKind::Structural { children, .. } = &mut p.kind {
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
    if let VKind::Structural { children, .. } = &mut p.kind {
        children.replace_child(old_child, new_child, new_intensity);
    }
}

fn remove_child_from_structural<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    parent: VNodeId,
    child: VNodeId,
) {
    let p = vnodes.get_mut(parent.index());
    if let VKind::Structural { children, .. } = &mut p.kind {
        children.remove_child(child);
    }
}

fn sole_sibling<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    parent: VNodeId,
    child: VNodeId,
) -> VNodeId {
    let p = vnodes.get(parent.index());
    if let VKind::Structural { children, .. } = &p.kind {
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
    if let VKind::Structural { children, .. } = &node.kind {
        let mut total = V::zero();
        for i in 0..children.len() {
            total = V::add(total, children.intensities[i]);
        }

        let _ = node;
        vnodes.get_mut(id.index()).intensity = total;
    }
}

fn propagate_v_sums_from<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, start: VNodeId) {
    recompute_structural_intensity(vnodes, start);
    let new_int = vnodes.get(start.index()).intensity;
    update_parent_cached_intensity(vnodes, start, new_int);
    propagate_v_sums(vnodes, start);
}

fn compute_has_evictable<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    children: &PackedChildren<V>,
) -> bool {
    for i in 0..children.len() {
        let (child_id, _) = children.get(i);
        let child = vnodes.get(child_id.index());
        let child_flag = match &child.kind {
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
    if let VKind::Structural { has_evictable, .. } = &mut node.kind {
        *has_evictable = flag;
    }
}

/// Returns `true` if `ancestor` is a strict ancestor of `descendant` in the V-tree
/// (i.e. reachable by following parent links from `descendant`).
pub(crate) fn is_ancestor<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    ancestor: VNodeId,
    mut descendant: VNodeId,
) -> bool {
    while let Some(p) = vnodes.get(descendant.index()).parent {
        if p == ancestor {
            return true;
        }
        descendant = p;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{invalidate_depth_subtree, propagate_v_sums, v_depth, vtree_remove_leaf};
    use crate::arena::Arena;
    use crate::handle::{GNodeId, VNodeId};
    use crate::nodes::vnode::{DEPTH_STALE, PackedChildren, VKind, VNode};

    fn entry_vnode(intensity: u32, parent: Option<VNodeId>) -> VNode<u32> {
        VNode {
            intensity,
            parent,
            cached_depth: AtomicU32::new(DEPTH_STALE),
            kind: VKind::Entry {
                gnode: GNodeId::from_index(0),
                is_exposed: true,
                is_evictable: true,
            },
        }
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
            let cached = vnodes.get(id.index()).cached_depth.load(Ordering::Relaxed);
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
            let mut node = entry_vnode(0, None);
            node.cached_depth = AtomicU32::new(0);
            let id = VNodeId::from_index(vnodes.alloc(node));

            invalidate_depth_subtree(&vnodes, id);

            let cached = vnodes.get(id.index()).cached_depth.load(Ordering::Relaxed);
            assert_eq!(cached, DEPTH_STALE);
        }

        #[test]
        fn leaves_already_stale_nodes_unchanged() {
            let mut vnodes: Arena<VNode<u32>> = Arena::new();
            // cached_depth is already DEPTH_STALE from entry_vnode helper
            let id = VNodeId::from_index(vnodes.alloc(entry_vnode(0, None)));
            invalidate_depth_subtree(&vnodes, id); // should not panic
            let cached = vnodes.get(id.index()).cached_depth.load(Ordering::Relaxed);
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

            let parent_node: VNode<u32> = VNode {
                intensity: 0,
                parent: None,
                cached_depth: AtomicU32::new(DEPTH_STALE),
                kind: VKind::Structural {
                    children: PackedChildren::new_2((child_a_id, 5u32), (child_b_id, 7u32)),
                    has_evictable: false,
                },
            };
            let parent_id = VNodeId::from_index(vnodes.alloc(parent_node));

            // Wire children back to parent
            vnodes.get_mut(child_a_id.index()).parent = Some(parent_id);
            vnodes.get_mut(child_b_id.index()).parent = Some(parent_id);

            // Update child_a's own intensity
            vnodes.get_mut(child_a_id.index()).intensity = 20;

            propagate_v_sums(&mut vnodes, child_a_id);

            // Parent intensity should now reflect sum of cached child intensities
            // (The structural node caches 5 and 7; propagate_v_sums recomputes from them)
            let parent_intensity = vnodes.get(parent_id.index()).intensity;
            assert_eq!(parent_intensity, 5 + 7); // cached intensities in PackedChildren
        }
    }

    // ── vtree_remove_leaf ─────────────────────────────────────────────────
    mod vtree_remove_leaf_fn {
        use std::sync::atomic::AtomicU32;

        use super::*;
        use crate::nodes::gnode::GNode;

        /// Minimal gnodes arena with one Terminal GNode at index 0.
        fn gnodes_with_one_node() -> Arena<GNode<u8, u32>> {
            let mut gnodes: Arena<GNode<u8, u32>> = Arena::new();
            gnodes.alloc(GNode {
                lo: 0u8,
                hi: 255u8,
                sum: 0u32,
                own: 0u32,
                left: None,
                right: None,
                parent: None,
                entry: None,
            });
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

            let parent_id = VNodeId::from_index(vnodes.alloc(VNode {
                intensity: 15,
                parent: None,
                cached_depth: AtomicU32::new(DEPTH_STALE),
                kind: VKind::Structural {
                    children: PackedChildren::new_3(
                        (child_a, 5u32),
                        (child_b, 5u32),
                        (child_c, 5u32),
                    ),
                    has_evictable: true,
                },
            }));

            vnodes.get_mut(child_a.index()).parent = Some(parent_id);
            vnodes.get_mut(child_b.index()).parent = Some(parent_id);
            vnodes.get_mut(child_c.index()).parent = Some(parent_id);

            let result = vtree_remove_leaf(&mut vnodes, &mut gnodes, child_c, Some(parent_id));
            assert_eq!(result, Some(parent_id)); // parent remains root
            assert!(!vnodes.is_occupied(child_c.index())); // target removed
            match &vnodes.get(parent_id.index()).kind {
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

            let parent_id = VNodeId::from_index(vnodes.alloc(VNode {
                intensity: 10,
                parent: None, // root — no grandparent
                cached_depth: AtomicU32::new(DEPTH_STALE),
                kind: VKind::Structural {
                    children: PackedChildren::new_2((target, 5u32), (sibling, 5u32)),
                    has_evictable: true,
                },
            }));

            vnodes.get_mut(target.index()).parent = Some(parent_id);
            vnodes.get_mut(sibling.index()).parent = Some(parent_id);

            let result = vtree_remove_leaf(&mut vnodes, &mut gnodes, target, Some(parent_id));
            assert_eq!(result, Some(sibling)); // sibling is new root
            assert!(!vnodes.is_occupied(target.index()));
            assert!(!vnodes.is_occupied(parent_id.index())); // parent collapsed
            assert!(vnodes.get(sibling.index()).parent.is_none());
        }
    }
}
