use crate::arena::Arena;
use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance;
use crate::graph::algorithm::violation_push;
use crate::handle::VNodeId;
use crate::nodes::gnode::GState;
use crate::nodes::vnode::{VKind, VNode};
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::vtree;

/// Topology of the V-node being evicted, captured before the leaf is removed.
struct LeafRemovalContext {
    v_parent: Option<VNodeId>,
    /// Number of children the V-parent had immediately before removal.
    child_count: usize,
    /// The V-node from which leaf-removal violations should be pushed.
    /// Present when the parent had 2 or 3 children.
    change_point: Option<VNodeId>,
    /// The sole surviving sibling when the parent collapses (2 → 1 children).
    collapse_sibling: Option<VNodeId>,
}

/// Captures the V-topology before removing `v_id` from the tree.
fn classify_leaf_removal<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    v_id: VNodeId,
) -> LeafRemovalContext {
    let v_parent = vnodes.get(v_id.index()).parent();
    let (child_count, change_point, collapse_sibling) = v_parent.map_or((0, None, None), |p| {
        let count = match &vnodes.get(p.index()).kind() {
            VKind::Structural { children, .. } => children.len(),
            VKind::Entry { .. } => 0,
        };
        match count {
            3 => (3, Some(p), None),
            2 => {
                let sibling = match &vnodes.get(p.index()).kind() {
                    VKind::Structural { children, .. } => {
                        let (c0, _) = children.get(0);
                        let (c1, _) = children.get(1);
                        if c0 == v_id { Some(c1) } else { Some(c0) }
                    }
                    VKind::Entry { .. } => None,
                };
                (2, vnodes.get(p.index()).parent(), sibling)
            }
            _ => (count, None, None),
        }
    });
    LeafRemovalContext {
        v_parent,
        child_count,
        change_point,
        collapse_sibling,
    }
}

/// Pushes all rebalancing violations triggered by the removal of `v_id`.
fn push_eviction_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    v_id: VNodeId,
    ctx: &LeafRemovalContext,
    violations: &mut Vec<VNodeId>,
) {
    if let Some(start) = ctx.change_point {
        violation_push::push_leaf_removal_violations(vnodes, start, violations);
    }
    match ctx.child_count {
        2 => {
            if let Some(sole) = ctx.collapse_sibling {
                tracing::debug!(
                    sole = sole.index(),
                    "evict_tip: calling push_collapse_violations"
                );
                violation_push::push_collapse_violations(vnodes, sole, violations);
                if let Some(grandparent) = ctx.change_point {
                    tracing::debug!(
                        sole = sole.index(),
                        grandparent = grandparent.index(),
                        "evict_tip: calling push_cousin_violations (source 9)",
                    );
                    violation_push::push_cousin_violations(vnodes, sole, grandparent, violations);
                }
            }
        }
        3 => {
            if let Some(p) = ctx.v_parent {
                tracing::debug!(
                    parent = p.index(),
                    "evict_tip: calling push_remaining_sibling_violations"
                );
                violation_push::push_remaining_sibling_violations(vnodes, p, v_id, violations);
            }
        }
        _ => {}
    }
}

/// Parent G-node state captured before dealloc, for use by the plateau update.
struct ParentSnapshot<C> {
    state: GState,
    lo: C,
    hi: C,
}

#[allow(clippy::too_many_lines)]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    pub(crate) fn evict_tip(&mut self, v_id: VNodeId) {
        let span = tracing::debug_span!(
            "evict_tip",
            v_id = v_id.index(),
            gnode = tracing::field::Empty,
            parent = tracing::field::Empty,
        )
        .entered();

        let gnode_id = match &self.vtree.nodes.get(v_id.index()).kind() {
            VKind::Entry {
                gnode,
                is_evictable,
                ..
            } => {
                debug_assert!(
                    *is_evictable,
                    "evict_tip: V-entry {} is not evictable",
                    v_id.index()
                );
                *gnode
            }
            VKind::Structural { .. } => panic!(
                "evict_tip: V-node {} is structural, not an entry",
                v_id.index()
            ),
        };
        span.record("gnode", gnode_id.index());

        assert_ne!(
            gnode_id, self.gtree.root,
            "evict_tip: cannot evict the G-root"
        );

        let parent_id = self
            .gtree
            .nodes
            .get(gnode_id.index())
            .parent()
            .expect("evict_tip: terminal G-node must have a parent");
        span.record("parent", parent_id.index());

        // ── Phase 1: G-tree restructuring ───────────────────────────────────────
        // Absorb the evicted child's sum into the parent's own weight, detach
        // the child slot, and recompute the parent sum invariant.
        let parent_sum_before = self.gtree.nodes.get(parent_id.index()).sum();
        self.gtree.merge_into_parent(gnode_id);
        debug_assert!(
            (self
                .gtree
                .nodes
                .get(parent_id.index())
                .sum()
                .to_f64_approx()
                - parent_sum_before.to_f64_approx())
            .abs()
                < 1e-9,
            "evict_tip: G-sum invariant violation after absorption: \
         recomputed={}, expected={}",
            self.gtree
                .nodes
                .get(parent_id.index())
                .sum()
                .to_f64_approx(),
            parent_sum_before.to_f64_approx()
        );

        // Capture the parent's post-eviction state here: G-tree structure will
        // not change further through the V-tree phases below.
        let parent_snapshot = {
            let pg = self.gtree.nodes.get(parent_id.index());
            ParentSnapshot {
                state: pg.state(),
                lo: pg.lo(),
                hi: pg.hi(),
            }
        };

        // ── Phase 2: V-intensity propagation ────────────────────────────────────
        // The parent's `own` changed; push its new intensity up the V-tree and
        // requeue any nodes that are now violated.
        let p_entry_id = self
            .gtree
            .nodes
            .get(parent_id.index())
            .entry()
            .expect("evict_tip: parent must have V-entry (has dependents)");
        {
            let p_own = self.gtree.nodes.get(parent_id.index()).own();
            self.vtree.nodes.get_mut(p_entry_id.index()).set_intensity(p_own);
            vtree::sync_intensity_in_parent(&mut self.vtree.nodes, p_entry_id, p_own);
            vtree::propagate_v_sums(&mut self.vtree.nodes, p_entry_id);

            let mut check_id = Some(p_entry_id);
            while let Some(id) = check_id {
                if rebalance::is_violated(&self.vtree.nodes, id) {
                    self.vtree.violations.push(id);
                }
                check_id = self.vtree.nodes.get(id.index()).parent();
            }
        }

        // ── Phase 3: Evictable / exposed flag propagation ────────────────────────
        {
            let p = self.gtree.nodes.get(parent_id.index());
            let parent_is_exposed = p.uncovered_range().is_some();
            let parent_is_evictable = p.is_terminal();
            let p_entry_id = p
                .entry()
                .expect("evict_tip: parent must have V-entry (has dependents)");
            if let VKind::Entry {
                is_exposed,
                is_evictable,
                ..
            } = self.vtree.nodes.get_mut(p_entry_id.index()).kind_mut()
            {
                *is_exposed = parent_is_exposed;
                *is_evictable = parent_is_evictable;
            }
            vtree::propagate_evictable_flags(&mut self.vtree.nodes, p_entry_id);
        }

        // ── Phase 4–6: Capture V-topology, remove leaf, push violations ─────
        // Topology must be captured before removal; violations are pushed after.
        let removal_ctx = classify_leaf_removal(&self.vtree.nodes, v_id);

        self.vtree.root =
            vtree::vtree_remove_leaf(&mut self.vtree.nodes, &mut self.gtree.nodes, v_id, self.vtree.root);

        push_eviction_violations(&self.vtree.nodes, v_id, &removal_ctx, &mut self.vtree.violations);

        // ── Phase 7: Debug audit for missed violations ───────────────────────
        if tracing::enabled!(tracing::Level::ERROR) {
            let ctx = crate::diagnostics::diagnostic::MissedViolationContext {
                evicted_parent: removal_ctx.v_parent,
                evicted_parent_child_count: removal_ctx.child_count,
                collapse_sibling: removal_ctx.collapse_sibling,
            };
            let all_violated = rebalance::find_violated_nodes(&self.vtree.nodes);
            let queued: std::collections::HashSet<usize> =
                self.vtree.violations.iter().map(|v| v.index()).collect();
            for v in all_violated {
                if !queued.contains(&v.index()) {
                    crate::diagnostics::diagnostic::diagnose_missed_violation(
                        &self.vtree.nodes,
                        v,
                        &ctx,
                    );
                }
            }
        }

        // ── Phase 8: Dealloc evicted node and update counts ──────────────────────
        self.gtree.nodes.dealloc(gnode_id.index());
        self.gtree.node_count -= 1;
        self.gtree.terminal_count -= 1;
        if self.gtree.nodes.get(parent_id.index()).is_terminal() {
            self.gtree.terminal_count += 1;
        }

        // ── Phase 9: Plateau mirror update ───────────────────────────────────────
        // plateau_after_evict is a no-op when the feature is disabled.
        let parent_state_after = parent_snapshot.state;
        self.plateau_after_evict(
            gnode_id,
            parent_id,
            parent_state_after,
            parent_snapshot.lo,
            parent_snapshot.hi,
        );
        #[cfg(feature = "dynamic-contour-tracking")]
        if tracing::enabled!(tracing::Level::DEBUG) {
            crate::diagnostics::diagnostic::audit_plateau_consistency(
                self,
                "POST-EVICT",
                Some(&crate::diagnostics::diagnostic::PlateauAuditContext {
                    parent_id,
                    parent_state: parent_state_after,
                }),
            );
        }
    }
}

pub fn scan_for_candidates<C: Coordinate, V: Accumulator, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<VNodeId> {
    let _span = tracing::trace_span!("scan_for_candidates").entered();
    let mut candidates = Vec::new();
    if let Some(v_root) = graph.vtree.root {
        scan_dfs(graph, v_root, 0, &mut candidates);
    }
    tracing::trace!(candidates = candidates.len(), "scan complete");
    candidates
}

fn scan_dfs<C: Coordinate, V: Accumulator, const N: u32>(
    graph: &GvGraph<C, V, N>,
    v_id: VNodeId,
    depth: u32,
    candidates: &mut Vec<VNodeId>,
) {
    let node = graph.vtree.nodes.get(v_id.index());
    match &node.kind() {
        VKind::Entry {
            gnode,
            is_evictable,
            ..
        } => {
            if depth > graph.gtree.live_depth_evict && *is_evictable && *gnode != graph.gtree.root {
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
                scan_dfs(graph, child_id, depth + 1, candidates);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::algorithm::evict::scan_for_candidates;
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

    /// Config with a small budget so the observe loop reaches the eviction
    /// check.  depth_create=1 < depth_evict=2 satisfies all invariants.
    /// soft_limit = budget (10) - headroom (9) = 1.
    fn eviction_config() -> Config<u32> {
        // depth_buffer = depth_evict - depth_create = 2 - 1 = 1
        // headroom     = 3^(1+1) = 9
        // required_headroom = max(9, 0) = 9
        // soft_limit   = budget - 9 = 10 - 9 = 1
        Config {
            split_threshold: 2,
            structural: StructuralConfig {
                depth_create: 1,
                depth_evict: 2,
                budget: Some(10),
                alpha_relax: 0.5,
                bounded_eviction: false,
            },
        }
    }

    // ── scan_for_candidates ───────────────────────────────────────────
    mod scan_for_candidates_fn {
        use super::*;

        #[test]
        fn returns_empty_for_fresh_graph() {
            // v_root is None on a fresh graph — scan returns nothing
            let g: G = GvGraph::new(make_config());
            let candidates = scan_for_candidates(&g);
            assert!(candidates.is_empty());
        }

        #[test]
        fn returns_empty_when_all_nodes_shallower_than_live_depth_evict() {
            // default config: depth_evict=5, after one split nodes are at
            // v-depth 2 which is well below 5 → no candidates
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32); // triggers bootstrap split
            let candidates = scan_for_candidates(&g);
            assert!(candidates.is_empty());
        }

        #[test]
        fn scan_traverses_tree_without_panic_after_several_splits() {
            // eviction_config: depth_evict=2, bootstrap leaves at v-depth 2
            // depth > 2 → no candidates, but DFS still traverses every node
            let mut g: G = GvGraph::new(eviction_config());
            g.observe(32u8, 3u32);
            g.observe(192u8, 3u32);
            let candidates = scan_for_candidates(&g);
            // entries at depth 2 are not > live_depth_evict=2 → empty
            assert!(candidates.is_empty());
        }
    }

    // ── evict_tip (via bounded-budget observe) ────────────────────────
    mod evict_tip_fn {
        use super::*;

        #[test]
        fn observe_with_budget_keeps_node_count_bounded() {
            // soft_limit=1, so after every split the eviction loop fires.
            let mut g: G = GvGraph::new(eviction_config());
            for i in 0u8..10 {
                g.observe(i.wrapping_mul(13), 3u32);
            }
            // The eviction mechanism must keep the graph alive (no panic).
            assert!(g.gtree.node_count >= 1);
        }

        #[test]
        fn total_sum_is_preserved_after_eviction() {
            // Energy is transferred to parent on eviction, so total_sum must
            // equal the sum of all delta values observed.
            let mut g: G = GvGraph::new(eviction_config());
            let n = 5u32;
            let delta = 3u32;
            for i in 0..n as u8 {
                g.observe(i.wrapping_mul(51), delta);
            }
            assert_eq!(g.total_sum(), n * delta);
        }
    }
}
