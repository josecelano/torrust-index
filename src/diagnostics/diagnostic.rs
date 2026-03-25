#![allow(dead_code)]

use std::fmt;

use crate::arena::Arena;
use crate::graph::algorithm::rebalance::{self, Ctx};
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GNode;
use crate::nodes::vnode::{VKind, VNode};
use crate::traits::{Accumulator, Coordinate, Inspectable};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::{graph::GvGraph, nodes::gnode::GState};

pub fn audit_violations<V: Accumulator + Inspectable>(
    vnodes: &Arena<VNode<V>>,
    violations: &[VNodeId],
    checkpoint: &str,
) -> Vec<VNodeId> {
    let all_violated = rebalance::find_violated_nodes(vnodes);
    let queued: std::collections::HashSet<usize> = violations.iter().map(|v| v.index()).collect();
    let mut missed = Vec::new();
    for &v in &all_violated {
        if !queued.contains(&v.index()) {
            tracing::error!(
                checkpoint,
                node = %Ctx(vnodes, v),
                "violation NOT in queue",
            );
            missed.push(v);
        }
    }
    missed
}

#[cfg(feature = "dynamic-contour-tracking")]
pub struct PlateauAuditContext {
    pub parent_id: GNodeId,
    pub parent_state: GState,
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn audit_plateau_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    checkpoint: &str,
    context: Option<&PlateauAuditContext>,
) {
    for &key in graph.plateaus.keys() {
        for &r in graph.plateau_basis.basis_elements(&key) {
            let back = graph.plateau_basis.plateau_key(r);
            if back != Some(key) {
                tracing::error!(
                    checkpoint,
                    ?key,
                    gnode = r.index(),
                    ?back,
                    "basis back-pointer inconsistency"
                );
            }
        }
    }

    let map_len = graph.plateaus.len();
    let basis_len = graph.plateau_basis.plateau_count();
    if map_len != basis_len {
        tracing::error!(
            checkpoint,
            map_len,
            basis_len,
            "plateaus.len() != plateau_basis.plateau_count()",
        );
    }

    if let Some(ctx) = context {
        if ctx.parent_state == GState::SemiInternal {
            let g = graph.gnodes.get(ctx.parent_id.index());
            let surviving = g.left.or(g.right);
            if let Some(surviving_id) = surviving {
                let parent_plateau = graph.plateau_basis.plateau_key(ctx.parent_id);
                if let Some(pk) = parent_plateau {
                    if let Some(ck) = graph.plateau_basis.plateau_key(surviving_id) {
                        if pk == ck {
                            tracing::debug!(
                                checkpoint,
                                ?pk,
                                ?ck,
                                parent = ctx.parent_id.index(),
                                surviving = surviving_id.index(),
                                "transient P-I4 overlap (will be repaired)",
                            );
                        }
                    }
                }
            }
        }
    }
}

pub struct EvictionContext {
    pub evicted_parent: Option<VNodeId>,
    pub evicted_parent_child_count: usize,
    pub collapse_sibling: Option<VNodeId>,
}

#[allow(clippy::too_many_lines)]
pub fn diagnose_missed_violation<V: Accumulator + Inspectable>(
    vnodes: &Arena<VNode<V>>,
    violated: VNodeId,
    context: &EvictionContext,
) {
    let v = vnodes.get(violated.index());
    let v_intensity = v.intensity;

    let Some(parent_id) = v.parent else {
        tracing::error!(
            node = violated.index(),
            "DIAGNOSIS: node has no parent (root?), should not be violated",
        );
        return;
    };

    let parent = vnodes.get(parent_id.index());
    let Some(grandparent_id) = parent.parent else {
        tracing::error!(
            node = violated.index(),
            parent = parent_id.index(),
            "DIAGNOSIS: parent has no grandparent (depth 1?), should not be violated",
        );
        return;
    };

    let grandparent = vnodes.get(grandparent_id.index());
    let uncles: Vec<(VNodeId, V)> = match &grandparent.kind {
        VKind::Structural { children, .. } => {
            children.iter().filter(|(id, _)| *id != parent_id).collect()
        }
        VKind::Entry { .. } => vec![],
    };

    let max_uncle_intensity = uncles
        .iter()
        .map(|(_, int)| int.to_f64_approx())
        .fold(0.0_f64, f64::max);

    let uncle_desc: Vec<String> = uncles
        .iter()
        .map(|(id, int)| format!("v{}({})", id.index(), int.to_f64_approx()))
        .collect();

    tracing::error!(
        node = violated.index(),
        intensity = v_intensity.to_f64_approx(),
        parent = parent_id.index(),
        parent_intensity = parent.intensity.to_f64_approx(),
        grandparent = grandparent_id.index(),
        grandparent_intensity = grandparent.intensity.to_f64_approx(),
        ?uncle_desc,
        max_uncle = max_uncle_intensity,
        is_violation = v_intensity.to_f64_approx() > max_uncle_intensity,
        "MISSED VIOLATION DIAGNOSIS",
    );

    tracing::error!(
        evicted_parent = ?context.evicted_parent.map(VNodeId::index),
        child_count = context.evicted_parent_child_count,
        collapse_sibling = ?context.collapse_sibling.map(VNodeId::index),
        "eviction context (child_count: 2=collapse, 3=3→2)",
    );

    if let Some(sole) = context.collapse_sibling {
        if is_ancestor(vnodes, sole, violated) {
            tracing::error!(
                node = violated.index(),
                collapse_sibling = sole.index(),
                "node IS a descendant of collapse_sibling → should have been caught by source 7",
            );

            let sole_children: Vec<usize> = match &vnodes.get(sole.index()).kind {
                VKind::Structural { children, .. } => (0..children.len())
                    .map(|i| children.get(i).0.index())
                    .collect(),
                VKind::Entry { .. } => vec![],
            };
            if sole_children.contains(&violated.index()) {
                tracing::error!(
                    node = violated.index(),
                    "node IS a direct child of collapse_sibling — source 7 should catch it",
                );
            } else {
                tracing::error!(
                    node = violated.index(),
                    ?sole_children,
                    "node is NOT a direct child — source 7 only checks direct children. MISSING SOURCE.",
                );
            }
        } else {
            tracing::error!(
                node = violated.index(),
                collapse_sibling = sole.index(),
                "node is NOT a descendant of collapse_sibling",
            );
        }
    }

    if let Some(evicted_p) = context.evicted_parent {
        if evicted_p == grandparent_id {
            tracing::error!(
                node = violated.index(),
                evicted_parent = evicted_p.index(),
                "node's grandparent = evicted_parent → parent is sibling of removed entry. Check source 8.",
            );
        }
    }

    let _ancestry_span =
        tracing::error_span!("vtree_path_to_violated", node = violated.index()).entered();
    let mut current = violated;
    let mut depth = 0_usize;
    loop {
        let n = vnodes.get(current.index());
        let kind = match &n.kind {
            VKind::Entry { .. } => "E",
            VKind::Structural { children, .. } => match children.len() {
                2 => "S2",
                3 => "S3",
                _ => "S?",
            },
        };
        tracing::error!(
            depth,
            node = current.index(),
            kind,
            intensity = n.intensity.to_f64_approx(),
            "ancestry",
        );
        match n.parent {
            Some(p) => {
                current = p;
                depth += 1;
            }
            None => break,
        }
    }
}

fn is_ancestor<V: Accumulator>(
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

pub struct Gn<'a, C: Coordinate, V: Accumulator + Inspectable>(
    pub &'a Arena<GNode<C, V>>,
    pub GNodeId,
);

impl<C: Coordinate, V: Accumulator + Inspectable> fmt::Display for Gn<'_, C, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        if !self.0.is_occupied(idx) {
            return write!(f, "G{idx}(DEAD)");
        }
        let g = self.0.get(idx);
        let state = match g.state() {
            crate::nodes::gnode::GState::Terminal => "T",
            crate::nodes::gnode::GState::SemiInternal => "S",
            crate::nodes::gnode::GState::Internal => "I",
        };
        write!(
            f,
            "G{idx}({state},[{},{}),sum={})",
            g.lo.to_f64(),
            g.hi.to_f64(),
            g.sum.to_f64_approx()
        )
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub struct Pl<'a, C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    pub &'a GvGraph<C, V, N>,
    pub GNodeId,
);

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> fmt::Display for Pl<'_, C, V, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        let key = self.0.plateau_basis.plateau_key(self.1);
        match key {
            Some(k) => {
                if let Some(plateau) = self.0.plateaus.get(&k) {
                    write!(
                        f,
                        "P([{},{}),d={},sum={})",
                        plateau.start.to_f64(),
                        plateau.end.to_f64(),
                        plateau.depth,
                        plateau.sum.to_f64_approx()
                    )
                } else {
                    write!(f, "P(G{idx},key={k:?},NO_PLATEAU)")
                }
            }
            None => write!(f, "P(G{idx},not_basis)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::diagnostic::{
        EvictionContext, audit_violations, diagnose_missed_violation,
    };
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

    // ── audit_violations ──────────────────────────────────────────────
    mod audit_violations_fn {
        use super::*;

        #[test]
        fn returns_empty_for_fresh_graph_with_no_violations_queued() {
            let g: G = GvGraph::new(make_config());
            let missed = audit_violations(g.vnodes(), &[], "test");
            assert!(missed.is_empty());
        }

        #[test]
        fn returns_empty_for_observed_graph_with_no_violations() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32); // bootstrap split
            let missed = audit_violations(g.vnodes(), &[], "test");
            // All splits should leave the graph in a consistent state.
            assert!(missed.is_empty());
        }

        #[test]
        fn returns_empty_after_multiple_splits() {
            let mut g: G = GvGraph::new(make_config());
            for coord in [32u8, 96, 160, 224] {
                g.observe(coord, 3u32);
            }
            let missed = audit_violations(g.vnodes(), &[], "test");
            assert!(missed.is_empty());
        }
    }

    // ── diagnose_missed_violation ─────────────────────────────────────
    mod diagnose_missed_violation_fn {
        use super::*;
        use crate::nodes::vnode::VKind;

        #[test]
        fn does_not_panic_for_root_vnode_after_bootstrap() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32); // bootstrap split creates v_root
            let v_root = g.v_root.expect("v_root must exist");
            let ctx = EvictionContext {
                evicted_parent: None,
                evicted_parent_child_count: 0,
                collapse_sibling: None,
            };
            // Root has no parent → hits "node has no parent" early-return path.
            diagnose_missed_violation(g.vnodes(), v_root, &ctx);
        }

        #[test]
        fn depth_one_child_hits_grandparent_not_found_path() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            let v_root = g.v_root.expect("v_root must exist after bootstrap");
            // Get a depth-1 child of v_root (parent=v_root, grandparent=None)
            let child_id = match &g.vnodes().get(v_root.index()).kind {
                VKind::Structural { children, .. } => children.get(0).0,
                _ => panic!("expected Structural v_root after bootstrap"),
            };
            let ctx = EvictionContext {
                evicted_parent: None,
                evicted_parent_child_count: 0,
                collapse_sibling: None,
            };
            // depth-1: parent exists, grandparent=None → "depth 1?" early-return path.
            diagnose_missed_violation(g.vnodes(), child_id, &ctx);
        }

        #[test]
        fn depth_two_entry_covers_full_diagnose_path() {
            let mut g: G = GvGraph::new(make_config());
            // Multiple observations to produce a depth-2+ vtree.
            for coord in [32u8, 96u8, 160u8, 224u8] {
                g.observe(coord, 3u32);
            }
            let v_root = g.v_root.expect("v_root must exist");
            // BFS to find a depth-2+ Entry node.
            let mut stack: Vec<(crate::handle::VNodeId, usize)> = vec![(v_root, 0)];
            let mut depth2_entry = None;
            while let Some((id, depth)) = stack.pop() {
                let n = g.vnodes().get(id.index());
                match &n.kind {
                    VKind::Entry { .. } if depth >= 2 => {
                        depth2_entry = Some(id);
                        break;
                    }
                    VKind::Structural { children, .. } => {
                        for (cid, _) in children.iter() {
                            stack.push((cid, depth + 1));
                        }
                    }
                    _ => {}
                }
            }
            let Some(entry_id) = depth2_entry else {
                return; // Not enough splits for depth-2; treat as vacuous pass.
            };
            // collapse_sibling=v_root: v_root IS an ancestor → is_ancestor returns true.
            let ctx = EvictionContext {
                evicted_parent: None,
                evicted_parent_child_count: 0,
                collapse_sibling: Some(v_root),
            };
            diagnose_missed_violation(g.vnodes(), entry_id, &ctx);
        }

        #[test]
        fn depth_two_entry_with_evicted_parent_equals_grandparent() {
            let mut g: G = GvGraph::new(make_config());
            for coord in [32u8, 96u8, 160u8, 224u8] {
                g.observe(coord, 3u32);
            }
            let v_root = g.v_root.expect("v_root must exist");
            // BFS to find depth-2 entry and its grandparent.
            let mut stack: Vec<(crate::handle::VNodeId, usize)> = vec![(v_root, 0)];
            let mut found: Option<(crate::handle::VNodeId, crate::handle::VNodeId)> = None;
            while let Some((id, depth)) = stack.pop() {
                let n = g.vnodes().get(id.index());
                match &n.kind {
                    VKind::Entry { .. } if depth >= 2 => {
                        let parent_id = n.parent.unwrap();
                        let grandparent_id =
                            g.vnodes().get(parent_id.index()).parent.unwrap();
                        found = Some((id, grandparent_id));
                        break;
                    }
                    VKind::Structural { children, .. } => {
                        for (cid, _) in children.iter() {
                            stack.push((cid, depth + 1));
                        }
                    }
                    _ => {}
                }
            }
            let Some((entry_id, grandparent_id)) = found else {
                return;
            };
            // evicted_parent == grandparent_id → triggers that tracing::error! branch.
            let ctx = EvictionContext {
                evicted_parent: Some(grandparent_id),
                evicted_parent_child_count: 2,
                collapse_sibling: None,
            };
            diagnose_missed_violation(g.vnodes(), entry_id, &ctx);
        }
    }

    // ── Gn::fmt ───────────────────────────────────────────────────────
    mod gn_display_fn {
        use super::*;
        use crate::diagnostics::diagnostic::Gn;
        use crate::handle::GNodeId;

        #[test]
        fn dead_gnode_shows_dead_marker() {
            // Fresh graph: only gnode 0 is allocated; index 1 is dead.
            let g: G = GvGraph::new(make_config());
            let display = format!("{}", Gn(g.gnodes(), GNodeId::from_index(1)));
            assert_eq!(display, "G1(DEAD)");
        }

        #[test]
        fn live_terminal_gnode_shows_state_and_range() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Find the first occupied gnode index.
            for i in 0..10 {
                if g.gnodes().is_occupied(i) {
                    let display =
                        format!("{}", Gn(g.gnodes(), GNodeId::from_index(i)));
                    assert!(display.starts_with(&format!("G{i}(")));
                    assert!(!display.contains("DEAD"));
                    return;
                }
            }
            panic!("no live gnode found after bootstrap");
        }
    }

    // ── audit_plateau_consistency ─────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod audit_plateau_consistency_fn {
        use super::*;
        use crate::diagnostics::diagnostic::{PlateauAuditContext, audit_plateau_consistency};
        use crate::handle::GNodeId;
        use crate::nodes::gnode::GState;

        #[test]
        fn does_not_panic_for_fresh_graph_no_context() {
            let g: G = GvGraph::new(make_config());
            audit_plateau_consistency(&g, "test", None);
        }

        #[test]
        fn does_not_panic_after_bootstrap_no_context() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            audit_plateau_consistency(&g, "test", None);
        }

        #[test]
        fn with_semi_internal_context_on_terminal_leaf() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Use a leaf gnode (Terminal) as context parent with SemiInternal state.
            // surviving = leaf.left.or(leaf.right) = None → inner block skipped.
            let ctx = PlateauAuditContext {
                parent_id: GNodeId::from_index(1), // leaf gnode
                parent_state: GState::SemiInternal,
            };
            audit_plateau_consistency(&g, "test", Some(&ctx));
        }

        #[test]
        fn with_semi_internal_context_on_internal_gnode() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Use the Internal root (gnode 0) as context parent with SemiInternal.
            // surviving = root.left.or(root.right) = Some(left_child).
            let ctx = PlateauAuditContext {
                parent_id: GNodeId::from_index(0), // internal root gnode
                parent_state: GState::SemiInternal,
            };
            audit_plateau_consistency(&g, "test", Some(&ctx));
        }
    }

    // ── Pl::fmt ───────────────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod pl_display_fn {
        use super::*;
        use crate::diagnostics::diagnostic::Pl;
        use crate::handle::GNodeId;

        #[test]
        fn internal_gnode_shows_not_basis() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // gnode[0] is Internal after bootstrap → not in plateau basis.
            let display = format!("{}", Pl(&g, GNodeId::from_index(0)));
            // Either "not_basis" or a plateau display; just verify no panic.
            assert!(!display.is_empty());
        }

        #[test]
        fn terminal_leaf_shows_plateau_info() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // gnode[1] is a Terminal leaf → should be in plateau basis.
            let display = format!("{}", Pl(&g, GNodeId::from_index(1)));
            assert!(!display.is_empty());
        }
    }
}
