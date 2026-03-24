use std::sync::atomic::Ordering;

use crate::arena::Arena;
use crate::nodes::gnode::GNode;
use crate::handle::VNodeId;
use crate::traits::{Accumulator, Coordinate};
use crate::nodes::vnode::{DEPTH_STALE, PackedChildren, VKind, VNode};

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
