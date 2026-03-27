#[cfg(feature = "dynamic-contour-tracking")]
use crate::diagnostics::plateau_invariants::{
    check_p_i1_i_keys_are_contour_steps, check_p_i1_ii_tile_contiguity,
    check_p_i1_iii_run_contains_tile, check_p_i2_basis_minimality, check_p_i3_basis_disjointness,
    check_p_i4_thatch_one_hop, check_p_i5_thatch_depth, check_plateau_basis_consistency,
    check_plateau_btreemap_key_consistency, check_plateau_depth_consistency,
    check_plateau_sum_consistency,
};
use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance::is_violated;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::vnode::VKind;
use crate::traits::{Accumulator, Coordinate, Inspectable};

pub use crate::diagnostics::dot::{dump_gtree_dot, dump_vtree_dot};
pub use crate::diagnostics::dump::{dump_gtree, dump_plateaus};

pub fn assert_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) {
    let errors = check_all_invariants(graph);
    if !errors.is_empty() {
        let msg = errors.join("\n");
        panic!("Invariant violations ({} total):\n{msg}", errors.len());
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(dead_code)]
pub fn check_plateau_only<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_plateau_invariants(graph, errors);
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(dead_code)]
pub fn check_p_i3_only<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_p_i3_basis_disjointness(graph, errors);
}

#[allow(dead_code)]
pub fn check_all_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    check_g_tree_invariants(graph, &mut errors);
    check_v_tree_invariants(graph, &mut errors);
    check_accounting_invariants(graph, &mut errors);
    #[cfg(feature = "dynamic-contour-tracking")]
    check_plateau_invariants(graph, &mut errors);

    errors
}

/// G-I1, G-I2, G-I4, G-I5: structural and summation invariants on the G-tree.
fn check_g_tree_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_g_i1_summation(graph, errors);
    check_g_i2_variable_fanout(graph, errors);
    check_g_i4_entry_consistency(graph, errors);
    check_g_i5_entry_bijection(graph, errors);
}

/// V-I1..V-I7: intensity summation, branching, uncle, flag, and leaf invariants.
fn check_v_tree_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_v_i1_structural_sum(graph, errors);
    check_v_i2_branching_factor(graph, errors);
    check_v_i3_max_uncle(graph, errors);
    check_v_i5_entry_leaf(graph, errors);
    check_v_i6_exposed_flag(graph, errors);
    check_v_i6b_evictable_flag(graph, errors);
    check_v_i7_structural_flag(graph, errors);
}

/// Accounting: parent links, root, node/terminal counts, depth gates, budget.
fn check_accounting_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_clean_accounting(graph, errors);
    check_parent_link_consistency(graph, errors);
    check_v_root_consistency(graph, errors);
    check_node_count_consistency(graph, errors);
    check_terminal_count_consistency(graph, errors);
    check_depth_gate_invariants(graph, errors);
    check_hard_budget(graph, errors);
}

/// P-I1..P-I5: plateau key, basis, sum, depth and thatch invariants.
/// Only compiled when the `dynamic-contour-tracking` feature is enabled.
#[cfg(feature = "dynamic-contour-tracking")]
fn check_plateau_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_plateau_btreemap_key_consistency(graph, errors);
    check_plateau_basis_consistency(graph, errors);
    check_plateau_sum_consistency(graph, errors);
    check_plateau_depth_consistency(graph, errors);
    check_p_i1_i_keys_are_contour_steps(graph, errors);
    check_p_i1_ii_tile_contiguity(graph, errors);
    check_p_i1_iii_run_contains_tile(graph, errors);
    check_p_i2_basis_minimality(graph, errors);
    check_p_i3_basis_disjointness(graph, errors);
    check_p_i4_thatch_one_hop(graph, errors);
    check_p_i5_thatch_depth(graph, errors);
}

#[allow(clippy::float_cmp)]
fn check_g_i1_summation<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        let left_sum = g
            .left()
            .map_or(0.0, |l| graph.gtree.nodes.get(l.index()).sum().to_f64_approx());
        let right_sum = g
            .right()
            .map_or(0.0, |r| graph.gtree.nodes.get(r.index()).sum().to_f64_approx());
        let expected = g.own().to_f64_approx() + left_sum + right_sum;
        let actual = g.sum().to_f64_approx();
        if expected != actual && (expected - actual).abs() > 1e-9 {
            errors.push(format!(
                "G-I1 violated at G-node {idx}: expected sum={expected}, actual sum={actual} \
                 (own={}, left_sum={left_sum}, right_sum={right_sum})",
                g.own().to_f64_approx()
            ));
        }
    }
}

fn check_g_i2_variable_fanout<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        let count = usize::from(g.left().is_some()) + usize::from(g.right().is_some());
        if count > 2 {
            errors.push(format!(
                "G-I2 violated at G-node {idx}: {count} children (max 2)"
            ));
        }
    }
}

fn check_g_i4_entry_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        if let Some(v_id) = g.entry() {
            if !graph.vnodes().is_occupied(v_id.index()) {
                errors.push(format!(
                    "G-I4 violated at G-node {idx}: entry V-node {} is not occupied",
                    v_id.index()
                ));
                continue;
            }
            let v = graph.vnodes().get(v_id.index());

            let g_own = g.own().to_f64_approx();
            let v_int = v.intensity().to_f64_approx();
            #[allow(clippy::float_cmp)]
            if g_own != v_int && (g_own - v_int).abs() > 1e-9 {
                errors.push(format!(
                    "G-I4 violated at G-node {idx}: g.own={g_own}, entry.intensity={v_int}"
                ));
            }

            match &v.kind() {
                VKind::Entry { gnode, .. } => {
                    let g_id = GNodeId::from_index(idx);
                    if *gnode != g_id {
                        errors.push(format!(
                            "G-I4 violated at G-node {idx}: entry's gnode={gnode:?}, expected {g_id:?}"
                        ));
                    }
                }
                VKind::Structural { .. } => {
                    errors.push(format!(
                        "G-I4 violated at G-node {idx}: entry is a structural V-node, not an entry"
                    ));
                }
            }
        }
    }
}

#[allow(clippy::float_cmp)]
/// G-I5: every occupied G-node must have exactly one V-Entry, and the total
/// count of occupied G-nodes must equal the total count of `VKind::Entry` nodes.
/// This asserts the 1-to-1 bijection between G-nodes and V-Entry nodes.
fn check_g_i5_entry_bijection<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let mut g_without_entry = Vec::new();
    let mut g_count = 0usize;
    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        g_count += 1;
        if g.entry().is_none() {
            g_without_entry.push(idx);
        }
    }

    for idx in &g_without_entry {
        errors.push(format!(
            "G-I5 violated at G-node {idx}: no V-Entry — every G-node must have exactly one"
        ));
    }

    let v_entry_count = graph
        .vnodes()
        .iter_occupied()
        .filter(|(_, v)| matches!(v.kind(), VKind::Entry { .. }))
        .count();

    if g_count != v_entry_count {
        errors.push(format!(
            "G-I5 violated: {g_count} occupied G-nodes but {v_entry_count} V-Entry nodes (must be equal)"
        ));
    }
}

fn check_v_i1_structural_sum<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Structural { children, .. } = &v.kind() {
            let mut sum = 0.0_f64;
            for i in 0..children.len() {
                let (child_id, cached_int) = children.get(i);

                if !graph.vnodes().is_occupied(child_id.index()) {
                    errors.push(format!(
                        "V-I1 violated at V-node {idx}: child {} is not occupied",
                        child_id.index()
                    ));
                    continue;
                }
                let actual_int = graph.vnodes().get(child_id.index()).intensity();
                if (cached_int.to_f64_approx() - actual_int.to_f64_approx()).abs() > 1e-9 {
                    errors.push(format!(
                        "V-I1 cached intensity mismatch at V-node {idx}, child {}: \
                         cached={}, actual={}",
                        child_id.index(),
                        cached_int.to_f64_approx(),
                        actual_int.to_f64_approx()
                    ));
                }
                sum += cached_int.to_f64_approx();
            }

            let node_int = v.intensity().to_f64_approx();
            if (node_int - sum).abs() > 1e-9 {
                errors.push(format!(
                    "V-I1 violated at V-node {idx}: intensity={node_int}, sum of children={sum}"
                ));
            }
        }
    }
}

fn check_v_i2_branching_factor<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Structural { children, .. } = &v.kind() {
            let len = children.len();
            if len != 2 && len != 3 {
                errors.push(format!(
                    "V-I2 violated at V-node {idx}: {len} children (must be 2 or 3)"
                ));
            }
        }
    }
}

fn check_v_i3_max_uncle<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        let v_id = VNodeId::from_index(idx);
        if is_violated(graph.vnodes(), v_id) {
            let int = v.intensity().to_f64_approx();
            let uncle =
                crate::graph::algorithm::rebalance::max_uncle_intensity(graph.vnodes(), v_id)
                    .map_or(f64::NAN, Inspectable::to_f64_approx);
            errors.push(format!(
                "V-I3 violated at V-node {idx}: intensity={int}, max_uncle={uncle}"
            ));
        }
    }
}

fn check_v_i5_entry_leaf<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Structural { children, .. } = &v.kind() {
            for i in 0..children.len() {
                let (child_id, _) = children.get(i);
                if !graph.vnodes().is_occupied(child_id.index()) {
                    errors.push(format!(
                        "V-I5 violated at V-node {idx}: child {} is not occupied",
                        child_id.index()
                    ));
                }
            }
        }
    }
}

fn check_v_i6_exposed_flag<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Entry {
            gnode, is_exposed, ..
        } = &v.kind()
        {
            if !graph.gtree.nodes.is_occupied(gnode.index()) {
                errors.push(format!(
                    "V-I6 violated at V-node {idx}: backing G-node {} is not occupied",
                    gnode.index()
                ));
                continue;
            }
            let g = graph.gtree.nodes.get(gnode.index());
            let expected = g.uncovered_range().is_some();
            if *is_exposed != expected {
                errors.push(format!(
                    "V-I6 violated at V-node {idx}: is_exposed={is_exposed}, \
                     expected={expected} (state={:?})",
                    g.state()
                ));
            }
        }
    }
}

fn check_v_i6b_evictable_flag<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Entry {
            gnode,
            is_evictable,
            is_exposed,
            ..
        } = &v.kind()
        {
            if !graph.gtree.nodes.is_occupied(gnode.index()) {
                continue;
            }
            let g = graph.gtree.nodes.get(gnode.index());
            let expected = g.is_terminal();
            if *is_evictable != expected {
                errors.push(format!(
                    "V-I6b violated at V-node {idx}: is_evictable={is_evictable}, \
                     expected={expected} (state={:?})",
                    g.state()
                ));
            }

            if *is_evictable && !*is_exposed {
                errors.push(format!(
                    "V-I6b violated at V-node {idx}: is_evictable=true but is_exposed=false"
                ));
            }
        }
    }
}

fn check_v_i7_structural_flag<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Structural {
            children,
            has_evictable,
        } = &v.kind()
        {
            let expected = (0..children.len()).any(|i| {
                let (child_id, _) = children.get(i);
                if !graph.vnodes().is_occupied(child_id.index()) {
                    return false;
                }
                let child = graph.vnodes().get(child_id.index());
                match &child.kind() {
                    VKind::Entry { is_evictable, .. } => *is_evictable,
                    VKind::Structural { has_evictable, .. } => *has_evictable,
                }
            });
            if *has_evictable != expected {
                errors.push(format!(
                    "V-I7 violated at V-node {idx}: has_evictable={has_evictable}, \
                     expected={expected}"
                ));
            }
        }
    }
}

#[allow(clippy::float_cmp)]
fn check_clean_accounting<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let mut total_v = 0.0_f64;
    for (_, v) in graph.vnodes().iter_occupied() {
        if matches!(v.kind(), VKind::Entry { .. }) {
            total_v += v.intensity().to_f64_approx();
        }
    }
    let g_root_sum = graph
        .gtree.nodes
        .get(graph.gtree.root.index())
        .sum()
        .to_f64_approx();
    if total_v != g_root_sum && (total_v - g_root_sum).abs() > 1e-9 {
        errors.push(format!(
            "Clean accounting violated: V-entry sum={total_v}, G-root sum={g_root_sum}"
        ));
    }
}

fn check_parent_link_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_g_parent_links(graph, errors);
    check_v_parent_links(graph, errors);
}

/// Verifies that every G-node's left and right children point back to it as
/// their parent.
fn check_g_parent_links<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        let g_id = GNodeId::from_index(idx);
        if let Some(left) = g.left() {
            if graph.gtree.nodes.is_occupied(left.index()) {
                let child_parent = graph.gtree.nodes.get(left.index()).parent();
                if child_parent != Some(g_id) {
                    errors.push(format!(
                        "G-parent link: G-node {idx}'s left child {}'s parent is {:?}, expected {g_id:?}",
                        left.index(),
                        child_parent
                    ));
                }
            }
        }
        if let Some(right) = g.right() {
            if graph.gtree.nodes.is_occupied(right.index()) {
                let child_parent = graph.gtree.nodes.get(right.index()).parent();
                if child_parent != Some(g_id) {
                    errors.push(format!(
                        "G-parent link: G-node {idx}'s right child {}'s parent is {:?}, expected {g_id:?}",
                        right.index(),
                        child_parent
                    ));
                }
            }
        }
    }
}

/// Verifies V-tree parent–child consistency: every structural child points back
/// to its parent, and every node's claimed parent lists it as a child.
fn check_v_parent_links<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        let v_id = VNodeId::from_index(idx);
        if let VKind::Structural { children, .. } = &v.kind() {
            for i in 0..children.len() {
                let (child_id, _) = children.get(i);
                if graph.vnodes().is_occupied(child_id.index()) {
                    let child_parent = graph.vnodes().get(child_id.index()).parent();
                    if child_parent != Some(v_id) {
                        errors.push(format!(
                            "V-parent link: V-node {idx}'s child {}'s parent is {:?}, expected {v_id:?}",
                            child_id.index(),
                            child_parent
                        ));
                    }
                }
            }
        }

        if let Some(p_id) = v.parent() {
            if graph.vnodes().is_occupied(p_id.index()) {
                let parent = graph.vnodes().get(p_id.index());
                if let VKind::Structural { children, .. } = &parent.kind() {
                    if children.find_index(v_id).is_none() {
                        errors.push(format!(
                            "V-parent link: V-node {idx} has parent {}, but parent does not list it as a child",
                            p_id.index()
                        ));
                    }
                } else {
                    errors.push(format!(
                        "V-parent link: V-node {idx} has parent {}, but parent is not structural",
                        p_id.index()
                    ));
                }
            }
        }
    }
}

fn check_v_root_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    if let Some(root_id) = graph.v_root() {
        if !graph.vnodes().is_occupied(root_id.index()) {
            errors.push(format!(
                "V-root consistency: v_root {} is not occupied",
                root_id.index()
            ));
            return;
        }
        let root = graph.vnodes().get(root_id.index());
        if root.parent().is_some() {
            errors.push(format!(
                "V-root consistency: v_root {} has parent {:?}, expected None",
                root_id.index(),
                root.parent()
            ));
        }
    }
}

fn check_node_count_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let actual = graph.gtree.nodes.count();
    let expected = graph.gtree.node_count;
    if actual != expected {
        errors.push(format!(
            "Node count: graph.gtree.node_count={expected}, arena count={actual}"
        ));
    }
}

fn check_terminal_count_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    #[allow(clippy::cast_possible_truncation)]
    let actual = graph
        .gtree.nodes
        .iter_occupied()
        .filter(|(_, g)| g.is_terminal())
        .count() as u32;
    let expected = graph.gtree.terminal_count;
    if actual != expected {
        errors.push(format!(
            "Terminal count: graph.gtree.terminal_count={expected}, arena walk={actual}"
        ));
    }
}

fn check_hard_budget<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    if let Some(budget) = graph.config().structural.budget {
        if graph.gtree.node_count as usize > budget {
            errors.push(format!(
                "Hard budget violated (ADR-M-018): node_count ({}) > budget ({})",
                graph.gtree.node_count,
                budget
            ));
        }
    }
}

fn check_depth_gate_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let d_create = graph.depth_create();
    let d_evict = graph.depth_evict();
    let buffer = graph.gtree.depth_buffer;

    if d_create >= d_evict {
        errors.push(format!(
            "D-I3: live_depth_create ({d_create}) must be < live_depth_evict ({d_evict})"
        ));
    }

    if d_evict < buffer + 1 {
        errors.push(format!(
            "D-I3 floor: live_depth_evict ({d_evict}) < depth_buffer ({buffer}) + 1"
        ));
    }

    if d_evict >= buffer && d_create != d_evict - buffer {
        errors.push(format!(
            "D-I3 buffer: live_depth_create ({d_create}) != live_depth_evict ({d_evict}) - depth_buffer ({buffer})"
        ));
    }
}
