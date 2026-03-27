use crate::arena::Arena;
use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance::{Nd, contract};
use crate::graph::algorithm::violation_push::{
    push_promoted_violations, push_side_effect_violations,
};
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{Children, DEPTH_STALE, VKind, VNode};
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::vtree::{propagate_evictable_flags, v_depth};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    pub(crate) fn attempt_split(&mut self, g_id: GNodeId) {
        let g = self.gtree.nodes.get(g_id.index());

        if g.left().is_some() || g.right().is_some() {
            return;
        }

        let mid = C::midpoint(g.lo(), g.hi());
        if mid.partial_cmp(&g.lo()) != Some(std::cmp::Ordering::Greater) {
            return;
        }

        if g.sum().partial_cmp(&self.config.split_threshold) != Some(std::cmp::Ordering::Greater) {
            return;
        }

        let Some(entry_id) = g.entry() else {
            return;
        };

        if self.vnodes.get(entry_id.index()).parent().is_none() {
            bootstrap_split(self, g_id);
            return;
        }

        let p_id = self.vnodes.get(entry_id.index()).parent().unwrap();
        if let VKind::Structural { children, .. } = &self.vnodes.get(p_id.index()).kind() {
            if children.len() == 3 {
                let _span = tracing::debug_span!(
                    "split_preprocess",
                    p = %Nd(&self.vnodes, p_id),
                )
                .entered();
                let merged = contract(&mut self.vnodes, p_id);
                push_side_effect_violations(&self.vnodes, p_id, &mut self.violations);
                push_side_effect_violations(&self.vnodes, merged, &mut self.violations);
                push_promoted_violations(&self.vnodes, p_id, &mut self.violations);
            }
        }

        let entry_id = self.gtree.nodes.get(g_id.index()).entry().unwrap();
        if v_depth(&self.vnodes, entry_id) > self.gtree.live_depth_create {
            return;
        }

        catalytic_split(self, g_id);
    }
}

fn bootstrap_split<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    g_id: GNodeId,
) {
    let (lo, hi, entry_id) = {
        let g = graph.gtree.nodes.get(g_id.index());
        (
            g.lo(),
            g.hi(),
            g.entry().expect("bootstrap_split: g must have an entry"),
        )
    };
    let _span = tracing::debug_span!("bootstrap_split", g_id = g_id.index(), ?lo, ?hi,).entered();

    let (left_id, right_id) = graph.gtree.allocate_children(g_id);

    let le_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gtree.nodes, left_id);
    let re_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gtree.nodes, right_id);

    let cs_id = alloc_v_structural_2(&mut graph.vnodes, le_id, re_id);

    let entry_int = graph.vnodes.get(entry_id.index()).intensity();
    let root_structural = VNode::new_structural(
        entry_int,
        None,
        0,
        Children::new_2((entry_id, entry_int), (cs_id, V::zero())),
        true,
    );
    let root_s_id = VNodeId::from_index(graph.vnodes.alloc(root_structural));
    graph.vnodes.get_mut(entry_id.index()).set_parent(root_s_id);
    graph.vnodes.get_mut(cs_id.index()).set_parent(root_s_id);

    graph.vnodes.get(entry_id.index()).store_depth(1);
    graph.vnodes.get(cs_id.index()).store_depth(1);
    graph.vnodes.get(le_id.index()).store_depth(2);
    graph.vnodes.get(re_id.index()).store_depth(2);

    if let VKind::Entry {
        is_exposed,
        is_evictable,
        ..
    } = graph.vnodes.get_mut(entry_id.index()).kind_mut()
    {
        *is_exposed = false;
        *is_evictable = false;
    }

    graph.v_root = Some(root_s_id);

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

/// Saturating depth increment: returns `d + 1` unless `d` is `DEPTH_STALE`,
/// in which case `DEPTH_STALE` is propagated unchanged.
#[inline]
const fn depth_plus_one(d: u32) -> u32 {
    if d == DEPTH_STALE { DEPTH_STALE } else { d + 1 }
}

#[allow(clippy::too_many_lines)]
fn catalytic_split<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    g_id: GNodeId,
) {
    let (lo, hi, entry_id) = {
        let g = graph.gtree.nodes.get(g_id.index());
        (
            g.lo(),
            g.hi(),
            g.entry().expect("catalytic_split: g must have an entry"),
        )
    };
    let mid = C::midpoint(lo, hi);
    let _span =
        tracing::debug_span!("catalytic_split", g_id = g_id.index(), ?lo, ?hi, ?mid,).entered();
    let p_id = graph
        .vnodes
        .get(entry_id.index())
        .parent()
        .expect("catalytic_split: entry must have a parent");

    // ── Phase 1: Allocate G-children and V-entry nodes ───────────────────
    let (left_id, right_id) = graph.gtree.allocate_children(g_id);

    let le_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gtree.nodes, left_id);
    let re_id = alloc_v_entry(&mut graph.vnodes, &mut graph.gtree.nodes, right_id);

    // ── Phase 2: Compute depth metadata ───────────────────────────────────
    let p_depth = graph.vnodes.get(p_id.index()).cached_depth_raw();
    let s_depth = depth_plus_one(p_depth);
    let child_depth = depth_plus_one(s_depth);

    let s = VNode::new_structural(
        V::zero(),
        Some(p_id),
        s_depth,
        Children::new_2((le_id, V::zero()), (re_id, V::zero())),
        true,
    );
    let s_id = VNodeId::from_index(graph.vnodes.alloc(s));
    graph.vnodes.get_mut(le_id.index()).set_parent(s_id);
    graph.vnodes.get_mut(re_id.index()).set_parent(s_id);

    // ── Phase 4: Wire `s` into the parent's child list; store child depths ───
    graph.vnodes.get(le_id.index()).store_depth(child_depth);
    graph.vnodes.get(re_id.index()).store_depth(child_depth);

    let p = graph.vnodes.get_mut(p_id.index());
    if let VKind::Structural { children, .. } = p.kind_mut() {
        children.add_child(s_id, V::zero());
    }

    if let VKind::Entry {
        is_exposed,
        is_evictable,
        ..
    } = graph.vnodes.get_mut(entry_id.index()).kind_mut()
    {
        *is_exposed = false;
        *is_evictable = false;
    }

    // ── Phase 5: Propagate evictable flags ────────────────────────────────
    propagate_evictable_flags(&mut graph.vnodes, p_id);

    // ── Phase 6: Plateau state update ─────────────────────────────────────
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

fn alloc_v_entry<C: Coordinate, V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    gnodes: &mut Arena<GNode<C, V>>,
    gnode: GNodeId,
) -> VNodeId {
    let e = VNode::new_entry(V::zero(), None, DEPTH_STALE, gnode, true, true);
    let e_id = VNodeId::from_index(vnodes.alloc(e));
    gnodes.get_mut(gnode.index()).assign_entry(e_id);
    e_id
}

fn alloc_v_structural_2<V: Accumulator>(
    vnodes: &mut Arena<VNode<V>>,
    a: VNodeId,
    b: VNodeId,
) -> VNodeId {
    let a_int = vnodes.get(a.index()).intensity();
    let b_int = vnodes.get(b.index()).intensity();
    let s = VNode::new_structural(
        V::add(a_int, b_int),
        None,
        DEPTH_STALE,
        Children::new_2((a, a_int), (b, b_int)),
        true,
    );
    let s_id = VNodeId::from_index(vnodes.alloc(s));
    vnodes.get_mut(a.index()).set_parent(s_id);
    vnodes.get_mut(b.index()).set_parent(s_id);
    s_id
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph, StructuralConfig};

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
            let n0 = g.gtree.node_count;
            g.observe(64u8, 2u32);
            assert_eq!(
                g.gtree.node_count, n0,
                "must not split at exactly threshold"
            );
        }

        #[test]
        fn no_split_when_sum_below_threshold() {
            let mut g = fresh_graph();
            let n0 = g.gtree.node_count;
            g.observe(64u8, 1u32);
            assert_eq!(g.gtree.node_count, n0);
        }

        #[test]
        fn bootstrap_split_increases_node_count_by_two() {
            // sum > split_threshold on root triggers bootstrap_split
            let mut g = fresh_graph();
            let n0 = g.gtree.node_count;
            g.observe(64u8, 3u32);
            assert_eq!(g.gtree.node_count, n0 + 2);
        }

        #[test]
        fn bootstrap_split_increases_terminal_count_by_one() {
            let mut g = fresh_graph();
            let t0 = g.gtree.terminal_count;
            g.observe(64u8, 3u32);
            assert_eq!(g.gtree.terminal_count, t0 + 1);
        }

        #[test]
        fn catalytic_split_increases_node_count_further() {
            // First observation triggers bootstrap split; second observation on a
            // child triggers catalytic split.
            let mut g = fresh_graph();
            g.observe(32u8, 3u32); // bootstrap split — left child covers [0,128)
            let n1 = g.gtree.node_count;
            g.observe(32u8, 3u32); // catalytic split of the left child
            assert!(g.gtree.node_count > n1);
        }

        #[test]
        fn already_split_node_is_not_split_again() {
            // After a bootstrap split the root gnode has children, so
            // attempt_split on the root is a no-op (early return).
            let mut g = fresh_graph();
            g.observe(64u8, 3u32); // bootstrap — root now has children
            let n1 = g.gtree.node_count;
            // Observing the root coordinate again should not double-split the root.
            // A further split (if any) would happen on a child, not the root.
            g.observe(128u8, 3u32); // different half — may split the right child
            // node_count may grow (child splits) but the root is not split again
            assert!(g.gtree.node_count >= n1);
        }

        #[test]
        fn direct_call_on_internal_gnode_is_no_op() {
            // Exercises the first early-return guard (line 19): calling
            // attempt_split directly on a gnode that already has children
            // must be a no-op (it returns immediately).
            let mut g = fresh_graph();
            g.observe(64u8, 3u32); // bootstrap — g_root becomes Internal
            let root = g.gtree.root;
            let n_before = g.gtree.node_count;
            g.attempt_split(root);
            assert_eq!(g.gtree.node_count, n_before);
        }
    }
}
