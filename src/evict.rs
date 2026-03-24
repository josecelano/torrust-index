#[cfg(feature = "dynamic-contour-tracking")]
use crate::gnode::GState;
use crate::graph::GvGraph;
use crate::handle::VNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::vnode::VKind;
use crate::{rebalance, vtree};

#[allow(clippy::too_many_lines)]
pub fn evict_tip<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    v_id: VNodeId,
) {
    let span = tracing::debug_span!(
        "evict_tip",
        v_id = v_id.index(),
        gnode = tracing::field::Empty,
        parent = tracing::field::Empty,
    )
    .entered();

    let gnode_id = match &graph.vnodes.get(v_id.index()).kind {
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

    assert_ne!(gnode_id, graph.g_root, "evict_tip: cannot evict the G-root");

    let parent_id = graph
        .gnodes
        .get(gnode_id.index())
        .parent
        .expect("evict_tip: terminal G-node must have a parent");
    span.record("parent", parent_id.index());

    let child_sum = graph.gnodes.get(gnode_id.index()).sum;
    let parent_sum_before = graph.gnodes.get(parent_id.index()).sum;

    graph.gnodes.get_mut(parent_id.index()).own =
        V::add(graph.gnodes.get(parent_id.index()).own, child_sum);

    {
        let p = graph.gnodes.get_mut(parent_id.index());
        if p.left == Some(gnode_id) {
            p.left = None;
        } else if p.right == Some(gnode_id) {
            p.right = None;
        } else {
            panic!(
                "evict_tip: G-node {} is not a child of parent {}",
                gnode_id.index(),
                parent_id.index()
            );
        }
    }

    {
        let p = graph.gnodes.get(parent_id.index());
        let left_sum = p
            .left
            .map_or_else(V::zero, |l| graph.gnodes.get(l.index()).sum);
        let right_sum = p
            .right
            .map_or_else(V::zero, |r| graph.gnodes.get(r.index()).sum);
        let recomputed = V::add(p.own, V::add(left_sum, right_sum));
        debug_assert!(
            (recomputed.to_f64_approx() - parent_sum_before.to_f64_approx()).abs() < 1e-9,
            "evict_tip: G-sum invariant violation after absorption: \
             recomputed={}, expected={}",
            recomputed.to_f64_approx(),
            parent_sum_before.to_f64_approx()
        );

        graph.gnodes.get_mut(parent_id.index()).sum = recomputed;
    }

    let p_entry_id = graph
        .gnodes
        .get(parent_id.index())
        .entry
        .expect("evict_tip: parent must have V-entry (has dependents)");
    {
        let p_own = graph.gnodes.get(parent_id.index()).own;
        graph.vnodes.get_mut(p_entry_id.index()).intensity = p_own;
        vtree::update_parent_cached_intensity(&mut graph.vnodes, p_entry_id, p_own);
        vtree::propagate_v_sums(&mut graph.vnodes, p_entry_id);

        let mut check_id = Some(p_entry_id);
        while let Some(id) = check_id {
            if rebalance::is_violated(&graph.vnodes, id) {
                graph.violations.push(id);
            }
            check_id = graph.vnodes.get(id.index()).parent;
        }
    }

    {
        let p = graph.gnodes.get(parent_id.index());
        let parent_is_exposed = p.uncovered_range().is_some();
        let parent_is_evictable = p.is_terminal();
        let p_entry_id = p
            .entry
            .expect("evict_tip: parent must have V-entry (has dependents)");
        if let VKind::Entry {
            is_exposed,
            is_evictable,
            ..
        } = &mut graph.vnodes.get_mut(p_entry_id.index()).kind
        {
            *is_exposed = parent_is_exposed;
            *is_evictable = parent_is_evictable;
        }
        vtree::propagate_evictable_flags(&mut graph.vnodes, p_entry_id);
    }

    let v_parent = graph.vnodes.get(v_id.index()).parent;
    let (child_count, change_point, collapse_sibling) = v_parent.map_or((0, None, None), |p| {
        let count = match &graph.vnodes.get(p.index()).kind {
            VKind::Structural { children, .. } => children.len(),
            VKind::Entry { .. } => 0,
        };
        match count {
            3 => (3, Some(p), None),
            2 => {
                let sibling = match &graph.vnodes.get(p.index()).kind {
                    VKind::Structural { children, .. } => {
                        let (c0, _) = children.get(0);
                        let (c1, _) = children.get(1);
                        if c0 == v_id { Some(c1) } else { Some(c0) }
                    }
                    VKind::Entry { .. } => None,
                };
                (2, graph.vnodes.get(p.index()).parent, sibling)
            }
            _ => (count, None, None),
        }
    });

    graph.v_root =
        vtree::vtree_remove_leaf(&mut graph.vnodes, &mut graph.gnodes, v_id, graph.v_root);

    if let Some(start) = change_point {
        rebalance::push_leaf_removal_violations(&graph.vnodes, start, &mut graph.violations);
    }

    match child_count {
        2 => {
            if let Some(sole) = collapse_sibling {
                tracing::debug!(
                    sole = sole.index(),
                    "evict_tip: calling push_collapse_violations"
                );
                rebalance::push_collapse_violations(&graph.vnodes, sole, &mut graph.violations);

                if let Some(grandparent) = change_point {
                    tracing::debug!(
                        sole = sole.index(),
                        grandparent = grandparent.index(),
                        "evict_tip: calling push_cousin_violations (source 9)",
                    );
                    rebalance::push_cousin_violations(
                        &graph.vnodes,
                        sole,
                        grandparent,
                        &mut graph.violations,
                    );
                }
            }
        }
        3 => {
            if let Some(p) = v_parent {
                tracing::debug!(
                    parent = p.index(),
                    "evict_tip: calling push_remaining_sibling_violations"
                );
                rebalance::push_remaining_sibling_violations(
                    &graph.vnodes,
                    p,
                    v_id,
                    &mut graph.violations,
                );
            }
        }
        _ => {}
    }

    if tracing::enabled!(tracing::Level::ERROR) {
        let ctx = crate::diagnostic::EvictionContext {
            evicted_parent: v_parent,
            evicted_parent_child_count: child_count,
            collapse_sibling,
        };
        let all_violated = rebalance::find_violated_nodes(&graph.vnodes);
        let queued: std::collections::HashSet<usize> =
            graph.violations.iter().map(|v| v.index()).collect();
        for v in all_violated {
            if !queued.contains(&v.index()) {
                crate::diagnostic::diagnose_missed_violation(&graph.vnodes, v, &ctx);
            }
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    let parent_snapshot = {
        let pg = graph.gnodes.get(parent_id.index());
        (pg.state(), pg.lo, pg.hi)
    };

    graph.gnodes.dealloc(gnode_id.index());
    graph.node_count -= 1;

    graph.terminal_count -= 1;
    if graph.gnodes.get(parent_id.index()).is_terminal() {
        graph.terminal_count += 1;
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    {
        let (parent_state_after, parent_lo, parent_hi) = parent_snapshot;
        plateau_after_evict(
            graph,
            gnode_id,
            parent_id,
            parent_state_after,
            parent_lo,
            parent_hi,
        );

        if tracing::enabled!(tracing::Level::DEBUG) {
            crate::diagnostic::audit_plateau_consistency(
                graph,
                "POST-EVICT",
                Some(&crate::diagnostic::PlateauAuditContext {
                    parent_id,
                    parent_state: parent_state_after,
                }),
            );
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(clippy::too_many_lines)]
fn plateau_after_evict<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &mut GvGraph<C, V, N>,
    gnode_id: crate::handle::GNodeId,
    parent_id: crate::handle::GNodeId,
    parent_state_after: GState,
    parent_lo: C,
    parent_hi: C,
) {
    let _span = tracing::debug_span!(
        "plateau_after_evict",
        gnode = gnode_id.index(),
        parent = parent_id.index(),
        ?parent_state_after,
    )
    .entered();

    let evicted_key = graph.plateau_basis.remove(gnode_id);

    let mut displaced: Vec<crate::handle::GNodeId> = Vec::new();
    let mut displaced_extra: Vec<(crate::handle::GNodeId, u32)> = Vec::new();

    let ancestor_key = if let Some(key) = graph.plateau_basis.remove(parent_id) {
        if parent_state_after == GState::SemiInternal {
            let g = graph.gnodes.get(parent_id.index());
            if let Some(sib) = g.left.or(g.right) {
                if let Some(ok) = graph.plateau_basis.remove(sib) {
                    graph.fixup_plateau(ok);
                }
                displaced.push(sib);
            }
        }

        Some(key)
    } else {
        let mut path: Vec<crate::handle::GNodeId> = vec![parent_id];
        let mut cur = graph.gnodes.get(parent_id.index()).parent;
        let mut found = None;

        let parent_key = {
            let pg = graph.gnodes.get(parent_id.index());
            crate::plateau::basis_edge_of(pg)
        };

        while let Some(anc) = cur {
            if let Some(&anc_key) = graph.plateau_basis.plateau_key(anc).as_ref() {
                let next_key = graph
                    .plateaus
                    .range((
                        std::ops::Bound::Excluded(anc_key),
                        std::ops::Bound::Unbounded,
                    ))
                    .next()
                    .map(|(&k, _)| k);
                let tile_covers =
                    parent_key >= anc_key && next_key.is_none_or(|nk| parent_key < nk);

                if tile_covers {
                    graph.plateau_basis.remove(anc);

                    if parent_state_after == GState::SemiInternal {
                        let g = graph.gnodes.get(parent_id.index());
                        if let Some(sib) = g.left.or(g.right) {
                            if let Some(ok) = graph.plateau_basis.remove(sib) {
                                graph.fixup_plateau(ok);
                            }
                            displaced.push(sib);
                        }
                    }

                    for &path_node in &path {
                        let par = graph
                            .gnodes
                            .get(path_node.index())
                            .parent
                            .expect("path node must have a parent");
                        let pg = graph.gnodes.get(par.index());
                        let sibling = if pg.left == Some(path_node) {
                            pg.right
                        } else {
                            pg.left
                        };
                        if let Some(sib_id) = sibling {
                            if displaced.contains(&sib_id) {
                                continue;
                            }
                            if let Some(ok) = graph.plateau_basis.remove(sib_id) {
                                graph.fixup_plateau(ok);
                            }
                            displaced.push(sib_id);
                        }
                    }

                    found = Some(anc_key);
                    break;
                }
            }
            path.push(anc);
            cur = graph.gnodes.get(anc.index()).parent;
        }

        found
    };

    if let Some(ek) = evicted_key {
        graph.fixup_plateau(ek);
    }
    if let Some(ak) = ancestor_key {
        if evicted_key != Some(ak) {
            graph.fixup_plateau(ak);
        }
    }

    let parent_depth = match parent_state_after {
        GState::Terminal | GState::SemiInternal => {
            crate::gtree::gnode_depth_from_interval(parent_lo, parent_hi, N)
        }
        GState::Internal => {
            unreachable!("evict_tip: parent cannot remain Internal after eviction")
        }
    };

    let parent_be = crate::plateau::BasisEdge(parent_lo);

    {
        let right_keys: Vec<crate::plateau::BasisEdge<C>> = graph
            .plateaus
            .range(crate::plateau::BasisEdge(parent_hi)..)
            .take_while(|(_, p)| p.start.total_cmp(&parent_hi) != std::cmp::Ordering::Greater)
            .filter(|(_, p)| p.depth == parent_depth)
            .map(|(&k, _)| k)
            .collect();
        for rk in right_keys {
            let members: Vec<crate::handle::GNodeId> = graph
                .plateau_basis
                .basis_elements(&rk)
                .iter()
                .copied()
                .collect();
            if !members.is_empty() {
                tracing::trace!(
                    ?rk,
                    parent_depth,
                    members = ?members.iter().map(|g| g.index()).collect::<Vec<_>>(),
                    "evacuating right-adjacent same-depth plateau for evict placement",
                );
                for &m in &members {
                    graph.plateau_basis.remove(m);
                }
                graph.plateaus.remove(&rk);
                for &m in &members {
                    graph.collect_subtree_basis_elements(m, &mut displaced_extra);
                }
            }
        }

        let left_keys: Vec<crate::plateau::BasisEdge<C>> = graph
            .plateaus
            .range(..parent_be)
            .rev()
            .take_while(|(_, p)| p.end.total_cmp(&parent_lo) != std::cmp::Ordering::Less)
            .filter(|(_, p)| p.depth == parent_depth)
            .map(|(&k, _)| k)
            .collect();
        for lk in left_keys {
            let members: Vec<crate::handle::GNodeId> = graph
                .plateau_basis
                .basis_elements(&lk)
                .iter()
                .copied()
                .collect();
            if !members.is_empty() {
                tracing::trace!(
                    ?lk,
                    parent_depth,
                    members = ?members.iter().map(|g| g.index()).collect::<Vec<_>>(),
                    "evacuating left-adjacent same-depth plateau for evict placement",
                );
                for &m in &members {
                    graph.plateau_basis.remove(m);
                }
                graph.plateaus.remove(&lk);
                for &m in &members {
                    graph.collect_subtree_basis_elements(m, &mut displaced_extra);
                }
            }
        }
    }

    let mut to_place = Vec::new();
    to_place.push((parent_id, parent_depth));
    for &sib_id in &displaced {
        graph.collect_subtree_basis_elements(sib_id, &mut to_place);
    }
    to_place.extend(displaced_extra);
    graph.place_sorted(&mut to_place);
}

pub fn scan_for_candidates<C: Coordinate, V: Accumulator, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<VNodeId> {
    let _span = tracing::trace_span!("scan_for_candidates").entered();
    let mut candidates = Vec::new();
    if let Some(v_root) = graph.v_root {
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
    let node = graph.vnodes.get(v_id.index());
    match &node.kind {
        VKind::Entry {
            gnode,
            is_evictable,
            ..
        } => {
            if depth > graph.live_depth_evict && *is_evictable && *gnode != graph.g_root {
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
