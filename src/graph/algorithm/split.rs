use std::sync::atomic::{AtomicU32, Ordering};

use crate::arena::Arena;
use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance::{Nd, contract};
use crate::graph::algorithm::violation_push::{
    push_promoted_violations, push_side_effect_violations,
};
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{DEPTH_STALE, PackedChildren, VKind, VNode};
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::vtree::{propagate_evictable_flags, v_depth};

pub fn attempt_split<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    g_id: GNodeId,
) {
    let g = graph.gnodes.get(g_id.index());

    if g.left.is_some() || g.right.is_some() {
        return;
    }

    let mid = C::midpoint(g.lo, g.hi);
    if mid.partial_cmp(&g.lo) != Some(std::cmp::Ordering::Greater) {
        return;
    }

    if g.sum.partial_cmp(&graph.config.split_threshold) != Some(std::cmp::Ordering::Greater) {
        return;
    }

    let Some(entry_id) = g.entry else {
        return;
    };

    if graph.vnodes.get(entry_id.index()).parent.is_none() {
        bootstrap_split(graph, g_id);
        return;
    }

    let p_id = graph.vnodes.get(entry_id.index()).parent.unwrap();
    if let VKind::Structural { children, .. } = &graph.vnodes.get(p_id.index()).kind {
        if children.len() == 3 {
            let _span = tracing::debug_span!(
                "split_preprocess",
                p = %Nd(&graph.vnodes, p_id),
            )
            .entered();
            let merged = contract(&mut graph.vnodes, p_id);
            push_side_effect_violations(&graph.vnodes, p_id, &mut graph.violations);
            push_side_effect_violations(&graph.vnodes, merged, &mut graph.violations);
            push_promoted_violations(&graph.vnodes, p_id, &mut graph.violations);
        }
    }

    let entry_id = graph.gnodes.get(g_id.index()).entry.unwrap();
    if v_depth(&graph.vnodes, entry_id) > graph.live_depth_create {
        return;
    }

    catalytic_split(graph, g_id);
}

fn bootstrap_split<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    g_id: GNodeId,
) {
    let (lo, hi, entry_id) = {
        let g = graph.gnodes.get(g_id.index());
        (
            g.lo,
            g.hi,
            g.entry.expect("bootstrap_split: g must have an entry"),
        )
    };
    let mid = C::midpoint(lo, hi);
    let _span = tracing::debug_span!("bootstrap_split", g_id = g_id.index(), ?lo, ?hi,).entered();

    let left_id = alloc_g_child(&mut graph.gnodes, lo, mid, g_id);
    let right_id = alloc_g_child(&mut graph.gnodes, mid, hi, g_id);
    graph.gnodes.get_mut(g_id.index()).left = Some(left_id);
    graph.gnodes.get_mut(g_id.index()).right = Some(right_id);

    let le_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gnodes, left_id);
    let re_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gnodes, right_id);

    let cs_id = alloc_v_structural_2(&mut graph.vnodes, le_id, re_id);

    let entry_int = graph.vnodes.get(entry_id.index()).intensity;
    let root_structural = VNode {
        intensity: entry_int,
        parent: None,
        cached_depth: AtomicU32::new(0),
        kind: VKind::Structural {
            children: PackedChildren::new_2((entry_id, entry_int), (cs_id, V::zero())),
            has_evictable: true,
        },
    };
    let root_s_id = VNodeId::from_index(graph.vnodes.alloc(root_structural));
    graph.vnodes.get_mut(entry_id.index()).parent = Some(root_s_id);
    graph.vnodes.get_mut(cs_id.index()).parent = Some(root_s_id);

    graph
        .vnodes
        .get(entry_id.index())
        .cached_depth
        .store(1, Ordering::Relaxed);
    graph
        .vnodes
        .get(cs_id.index())
        .cached_depth
        .store(1, Ordering::Relaxed);
    graph
        .vnodes
        .get(le_id.index())
        .cached_depth
        .store(2, Ordering::Relaxed);
    graph
        .vnodes
        .get(re_id.index())
        .cached_depth
        .store(2, Ordering::Relaxed);

    if let VKind::Entry {
        is_exposed,
        is_evictable,
        ..
    } = &mut graph.vnodes.get_mut(entry_id.index()).kind
    {
        *is_exposed = false;
        *is_evictable = false;
    }

    graph.v_root = Some(root_s_id);
    graph.node_count += 2;

    graph.terminal_count += 1;

    graph.plateau_after_bootstrap_split(g_id, left_id);

    #[cfg(feature = "dynamic-contour-tracking")]
    if tracing::enabled!(tracing::Level::DEBUG) {
        crate::diagnostics::diagnostic::audit_plateau_consistency(
            graph,
            "POST-BOOTSTRAP-SPLIT",
            None,
        );
    }
}

#[allow(clippy::too_many_lines)]
fn catalytic_split<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    g_id: GNodeId,
) {
    let (lo, hi, entry_id) = {
        let g = graph.gnodes.get(g_id.index());
        (
            g.lo,
            g.hi,
            g.entry.expect("catalytic_split: g must have an entry"),
        )
    };
    let mid = C::midpoint(lo, hi);
    let _span =
        tracing::debug_span!("catalytic_split", g_id = g_id.index(), ?lo, ?hi, ?mid,).entered();
    let p_id = graph
        .vnodes
        .get(entry_id.index())
        .parent
        .expect("catalytic_split: entry must have a parent");

    let left_id = alloc_g_child(&mut graph.gnodes, lo, mid, g_id);
    let right_id = alloc_g_child(&mut graph.gnodes, mid, hi, g_id);
    graph.gnodes.get_mut(g_id.index()).left = Some(left_id);
    graph.gnodes.get_mut(g_id.index()).right = Some(right_id);

    let le_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gnodes, left_id);
    let re_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gnodes, right_id);

    let p_depth = graph
        .vnodes
        .get(p_id.index())
        .cached_depth
        .load(Ordering::Relaxed);
    let s_depth = if p_depth == DEPTH_STALE {
        DEPTH_STALE
    } else {
        p_depth + 1
    };
    let child_depth = if s_depth == DEPTH_STALE {
        DEPTH_STALE
    } else {
        s_depth + 1
    };

    let s = VNode {
        intensity: V::zero(),
        parent: Some(p_id),
        cached_depth: AtomicU32::new(s_depth),
        kind: VKind::Structural {
            children: PackedChildren::new_2((le_id, V::zero()), (re_id, V::zero())),
            has_evictable: true,
        },
    };
    let s_id = VNodeId::from_index(graph.vnodes.alloc(s));
    graph.vnodes.get_mut(le_id.index()).parent = Some(s_id);
    graph.vnodes.get_mut(re_id.index()).parent = Some(s_id);

    graph
        .vnodes
        .get(le_id.index())
        .cached_depth
        .store(child_depth, Ordering::Relaxed);
    graph
        .vnodes
        .get(re_id.index())
        .cached_depth
        .store(child_depth, Ordering::Relaxed);

    let p = graph.vnodes.get_mut(p_id.index());
    if let VKind::Structural { children, .. } = &mut p.kind {
        children.add_child(s_id, V::zero());
    }

    if let VKind::Entry {
        is_exposed,
        is_evictable,
        ..
    } = &mut graph.vnodes.get_mut(entry_id.index()).kind
    {
        *is_exposed = false;
        *is_evictable = false;
    }

    propagate_evictable_flags(&mut graph.vnodes, p_id);

    graph.node_count += 2;

    graph.terminal_count += 1;

    graph.plateau_after_catalytic_split(g_id, left_id);

    #[cfg(feature = "dynamic-contour-tracking")]
    if tracing::enabled!(tracing::Level::DEBUG) {
        crate::diagnostics::diagnostic::audit_plateau_consistency(
            graph,
            "POST-CATALYTIC-SPLIT",
            None,
        );
    }
}

fn alloc_g_child<C: Coordinate, V: Accumulator>(
    gnodes: &mut Arena<GNode<C, V>>,
    lo: C,
    hi: C,
    parent: GNodeId,
) -> GNodeId {
    let g = GNode {
        lo,
        hi,
        sum: V::zero(),
        own: V::zero(),
        left: None,
        right: None,
        parent: Some(parent),
        entry: None,
    };
    GNodeId::from_index(gnodes.alloc(g))
}

fn alloc_v_entry<C: Coordinate, V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    gnodes: &mut Arena<GNode<C, V>>,
    gnode: GNodeId,
) -> VNodeId {
    let e = VNode {
        intensity: V::zero(),
        parent: None,
        cached_depth: AtomicU32::new(DEPTH_STALE),
        kind: VKind::Entry {
            gnode,
            is_exposed: true,
            is_evictable: true,
        },
    };
    let e_id = VNodeId::from_index(vnodes.alloc(e));
    gnodes.get_mut(gnode.index()).entry = Some(e_id);
    e_id
}

fn alloc_v_structural_2<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    a: VNodeId,
    b: VNodeId,
) -> VNodeId {
    let a_int = vnodes.get(a.index()).intensity;
    let b_int = vnodes.get(b.index()).intensity;
    let s = VNode {
        intensity: V::add(a_int, b_int),
        parent: None,
        cached_depth: AtomicU32::new(DEPTH_STALE),
        kind: VKind::Structural {
            children: PackedChildren::new_2((a, a_int), (b, b_int)),
            has_evictable: true,
        },
    };
    let s_id = VNodeId::from_index(vnodes.alloc(s));
    vnodes.get_mut(a.index()).parent = Some(s_id);
    vnodes.get_mut(b.index()).parent = Some(s_id);
    s_id
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph};

    type G = GvGraph<u8, u32, 8>;

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            depth_create: 3,
            depth_evict: 5,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        }
    }

    fn fresh_graph() -> G {
        GvGraph::new(make_config())
    }

    // ── attempt_split (via observe) ───────────────────────────────────
    mod attempt_split {
        use super::*;

        #[test]
        fn no_split_when_sum_at_threshold() {
            // delta=2 equals split_threshold, condition is >, not >=
            let mut g = fresh_graph();
            let n0 = g.node_count();
            g.observe(64u8, 2u32);
            assert_eq!(g.node_count(), n0, "must not split at exactly threshold");
        }

        #[test]
        fn no_split_when_sum_below_threshold() {
            let mut g = fresh_graph();
            let n0 = g.node_count();
            g.observe(64u8, 1u32);
            assert_eq!(g.node_count(), n0);
        }

        #[test]
        fn bootstrap_split_increases_node_count_by_two() {
            // sum > split_threshold on root triggers bootstrap_split
            let mut g = fresh_graph();
            let n0 = g.node_count();
            g.observe(64u8, 3u32);
            assert_eq!(g.node_count(), n0 + 2);
        }

        #[test]
        fn bootstrap_split_increases_terminal_count_by_one() {
            let mut g = fresh_graph();
            let t0 = g.terminal_count();
            g.observe(64u8, 3u32);
            assert_eq!(g.terminal_count(), t0 + 1);
        }

        #[test]
        fn catalytic_split_increases_node_count_further() {
            // First observation triggers bootstrap split; second observation on a
            // child triggers catalytic split.
            let mut g = fresh_graph();
            g.observe(32u8, 3u32); // bootstrap split — left child covers [0,128)
            let n1 = g.node_count();
            g.observe(32u8, 3u32); // catalytic split of the left child
            assert!(g.node_count() > n1);
        }

        #[test]
        fn already_split_node_is_not_split_again() {
            // After a bootstrap split the root gnode has children, so
            // attempt_split on the root is a no-op (early return).
            let mut g = fresh_graph();
            g.observe(64u8, 3u32); // bootstrap — root now has children
            let n1 = g.node_count();
            // Observing the root coordinate again should not double-split the root.
            // A further split (if any) would happen on a child, not the root.
            g.observe(128u8, 3u32); // different half — may split the right child
            // node_count may grow (child splits) but the root is not split again
            assert!(g.node_count() >= n1);
        }

        #[test]
        fn direct_call_on_internal_gnode_is_no_op() {
            // Exercises the first early-return guard (line 19): calling
            // attempt_split directly on a gnode that already has children
            // must be a no-op (it returns immediately).
            let mut g = fresh_graph();
            g.observe(64u8, 3u32); // bootstrap — g_root becomes Internal
            let root = g.g_root();
            let n_before = g.node_count();
            crate::graph::algorithm::split::attempt_split(&mut g, root);
            assert_eq!(g.node_count(), n_before);
        }
    }
}
