//! Promote operations — V-tree restructuring that increases tree height.
//!
//! All three promote variants are collected here because they share the same
//! structural goal (lifting nodes to reduce violations) while differing in
//! which nodes are moved and whether a new G-node must be created.
//!
//! - [`standard_promote`]: 2-child structural node absorbs into its parent
//!   (pure V-tree, no G-tree mutation).
//! - [`skip_promote`]: entry node skips its parent and joins the grandparent
//!   (pure V-tree, no G-tree mutation).
//! - [`legacy_promote`]: semi-internal G-node expands by allocating a new
//!   G-child, which is the **only** case in the rebalancing path that mutates
//!   the G-tree arena.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::arena::Arena;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{DEPTH_STALE, PackedChildren, VKind, VNode};
use crate::traits::{Accumulator, Coordinate};
use crate::tree::vtree::{
    invalidate_depth_subtree, propagate_evictable_flags, recompute_structural_intensity,
    replace_child_in_parent, sync_intensity_in_parent,
};

use super::rebalance::{Ch, Nd, node_has_evictable};

pub fn standard_promote<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, c: VNodeId) {
    let p = vnodes
        .get(c.index())
        .parent
        .expect("standard_promote: c must have a parent");
    let _span = tracing::debug_span!(
        "standard_promote",
        c = %Nd(vnodes, c),
        children = %Ch(vnodes, c),
    )
    .entered();

    let (c1_id, c1_int, c2_id, c2_int) = {
        let node = vnodes.get(c.index());
        match &node.kind {
            VKind::Structural { children, .. } => {
                assert!(children.len() == 2, "standard_promote: c must be a 2-node");
                let (id1, int1) = children.get(0);
                let (id2, int2) = children.get(1);
                (id1, int1, id2, int2)
            }
            VKind::Entry { .. } => panic!("standard_promote: c must be structural"),
        }
    };

    let sibling_id = {
        let p_node = vnodes.get(p.index());
        match &p_node.kind {
            VKind::Structural { children, .. } => {
                let mut sib = None;
                for i in 0..children.len() {
                    let (id, _) = children.get(i);
                    if id != c {
                        sib = Some(children.get(i));
                        break;
                    }
                }
                sib.expect("standard_promote: sibling not found")
            }
            VKind::Entry { .. } => panic!("standard_promote: parent must be structural"),
        }
    };

    let sib_terminal = node_has_evictable(vnodes, sibling_id.0);
    let c1_terminal = node_has_evictable(vnodes, c1_id);
    let c2_terminal = node_has_evictable(vnodes, c2_id);

    let p_node = vnodes.get_mut(p.index());
    if let VKind::Structural {
        children,
        has_evictable,
    } = &mut p_node.kind
    {
        *children = PackedChildren::new_3((c1_id, c1_int), (c2_id, c2_int), sibling_id);
        *has_evictable = c1_terminal || c2_terminal || sib_terminal;
    }

    vnodes.get_mut(c1_id.index()).parent = Some(p);
    vnodes.get_mut(c2_id.index()).parent = Some(p);

    invalidate_depth_subtree(vnodes, c1_id);
    invalidate_depth_subtree(vnodes, c2_id);

    vnodes.dealloc(c.index());

    propagate_evictable_flags(vnodes, p);

    tracing::debug!(result = %Ch(vnodes, p), "c destroyed, p is 3-node");
}

pub fn skip_promote<V: Accumulator>(vnodes: &mut Arena<VNode<V>>, c: VNodeId) -> Option<VNodeId> {
    let p = vnodes
        .get(c.index())
        .parent
        .expect("skip_promote: c must have a parent");
    let g = vnodes
        .get(p.index())
        .parent
        .expect("skip_promote: p must have a grandparent");
    let _span = tracing::debug_span!(
        "skip_promote",
        c = %Nd(vnodes, c),
        p = p.index(),
        g = g.index(),
    )
    .entered();

    let (s_id, s_int) = {
        let p_node = vnodes.get(p.index());
        match &p_node.kind {
            VKind::Structural { children, .. } => {
                let mut sib = None;
                for i in 0..children.len() {
                    let (id, int) = children.get(i);
                    if id != c {
                        sib = Some((id, int));
                        break;
                    }
                }
                sib.expect("skip_promote: sibling not found")
            }
            VKind::Entry { .. } => panic!("skip_promote: parent must be structural"),
        }
    };

    let c_int = vnodes.get(c.index()).intensity;

    let (u_id, u_int) = {
        let g_node = vnodes.get(g.index());
        match &g_node.kind {
            VKind::Structural { children, .. } => {
                let mut uncle = None;
                for i in 0..children.len() {
                    let (id, int) = children.get(i);
                    if id != p {
                        uncle = Some((id, int));
                        break;
                    }
                }
                uncle.expect("skip_promote: uncle not found")
            }
            VKind::Entry { .. } => panic!("skip_promote: grandparent must be structural"),
        }
    };

    let c_terminal = node_has_evictable(vnodes, c);
    let s_terminal = node_has_evictable(vnodes, s_id);
    let u_terminal = node_has_evictable(vnodes, u_id);

    let g_node = vnodes.get_mut(g.index());
    if let VKind::Structural {
        children,
        has_evictable,
    } = &mut g_node.kind
    {
        *children = PackedChildren::new_3((c, c_int), (s_id, s_int), (u_id, u_int));
        *has_evictable = c_terminal || s_terminal || u_terminal;
    }

    vnodes.get_mut(c.index()).parent = Some(g);
    vnodes.get_mut(s_id.index()).parent = Some(g);

    invalidate_depth_subtree(vnodes, c);
    invalidate_depth_subtree(vnodes, s_id);

    vnodes.dealloc(p.index());

    propagate_evictable_flags(vnodes, g);

    tracing::debug!(result = %Ch(vnodes, g), "p destroyed, g is 3-node");

    None
}

#[allow(clippy::too_many_lines)]
pub fn legacy_promote<C: Coordinate, V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    gnodes: &mut Arena<GNode<C, V>>,
    c: VNodeId,
) -> GNodeId {
    let p = vnodes
        .get(c.index())
        .parent
        .expect("legacy_promote: c must have a parent");
    let g = vnodes
        .get(p.index())
        .parent
        .expect("legacy_promote: p must have a grandparent");

    let gnode_id = match &vnodes.get(c.index()).kind {
        VKind::Entry { gnode, .. } => *gnode,
        VKind::Structural { .. } => panic!("legacy_promote: c must be an entry"),
    };

    debug_assert!(
        gnodes.get(gnode_id.index()).is_semi_internal(),
        "legacy_promote: backing G-node must be semi-internal"
    );

    let _span = tracing::debug_span!(
        "legacy_promote",
        c = %Nd(vnodes, c),
        p = p.index(),
        g = g.index(),
        gnode = gnode_id.index(),
    )
    .entered();

    let gn = gnodes.get(gnode_id.index());
    let (new_lo, new_hi) = gn
        .uncovered_range()
        .expect("legacy_promote: semi-internal must have uncovered range");
    let new_child = GNode {
        lo: new_lo,
        hi: new_hi,
        sum: V::zero(),
        own: V::zero(),
        left: None,
        right: None,
        parent: Some(gnode_id),
        entry: None,
    };
    let new_child_id = GNodeId::from_index(gnodes.alloc(new_child));

    {
        let gn = gnodes.get_mut(gnode_id.index());
        if gn.left.is_none() {
            gn.left = Some(new_child_id);
        } else {
            debug_assert!(
                gn.right.is_none(),
                "legacy_promote: expected empty right slot"
            );
            gn.right = Some(new_child_id);
        }
    }

    let c_depth = vnodes.get(c.index()).cached_depth.load(Ordering::Relaxed);
    let ne = VNode {
        intensity: V::zero(),
        parent: Some(p),
        cached_depth: AtomicU32::new(c_depth),
        kind: VKind::Entry {
            gnode: new_child_id,
            is_exposed: true,
            is_evictable: true,
        },
    };
    let ne_id = VNodeId::from_index(vnodes.alloc(ne));
    gnodes.get_mut(new_child_id.index()).entry = Some(ne_id);

    let c_int = vnodes.get(c.index()).intensity;
    replace_child_in_parent(vnodes, p, c, ne_id, V::zero());

    let (u_id, u_int) = sibling_of(vnodes, g, p);

    let c_evictable = false;
    let p_evictable = node_has_evictable(vnodes, p);
    let u_evictable = node_has_evictable(vnodes, u_id);

    let p_int = vnodes.get(p.index()).intensity;
    let g_node = vnodes.get_mut(g.index());
    if let VKind::Structural {
        children,
        has_evictable,
    } = &mut g_node.kind
    {
        *children = PackedChildren::new_3((c, c_int), (p, p_int), (u_id, u_int));
        *has_evictable = c_evictable || p_evictable || u_evictable;
    }

    vnodes.get_mut(c.index()).parent = Some(g);

    let new_c_depth = if c_depth == DEPTH_STALE {
        DEPTH_STALE
    } else {
        c_depth - 1
    };
    vnodes
        .get(c.index())
        .cached_depth
        .store(new_c_depth, Ordering::Relaxed);

    if let VKind::Entry {
        is_exposed,
        is_evictable,
        ..
    } = &mut vnodes.get_mut(c.index()).kind
    {
        *is_exposed = false;
        *is_evictable = false;
    }

    recompute_structural_intensity(vnodes, p);
    let new_p_int = vnodes.get(p.index()).intensity;
    sync_intensity_in_parent(vnodes, p, new_p_int);

    propagate_evictable_flags(vnodes, p);
    propagate_evictable_flags(vnodes, g);

    tracing::debug!(
        new_gnode = new_child_id.index(),
        new_ventry = ne_id.index(),
        "legacy_promote complete: c lifted to g, new child created",
    );

    new_child_id
}

fn sibling_of<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    parent: VNodeId,
    child: VNodeId,
) -> (VNodeId, V) {
    let p_node = vnodes.get(parent.index());
    match &p_node.kind {
        VKind::Structural { children, .. } => {
            for i in 0..children.len() {
                let (id, int) = children.get(i);
                if id != child {
                    return (id, int);
                }
            }
            panic!("sibling_of: child not found in parent");
        }
        VKind::Entry { .. } => panic!("sibling_of: parent must be structural"),
    }
}
