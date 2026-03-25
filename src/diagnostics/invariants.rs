use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance::is_violated;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::graph::uniform_contour_depth_of;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::GState;
use crate::nodes::vnode::VKind;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau::BasisEdge;
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::gtree::gnode_depth_from_interval;

const fn state_label(s: GState) -> &'static str {
    match s {
        GState::Terminal => "T",
        GState::SemiInternal => "S",
        GState::Internal => "I",
    }
}

fn fmt_optional_gnode(opt: Option<GNodeId>, fallback: &str) -> String {
    opt.map_or_else(|| fallback.to_string(), |id| format!("{}", id.index()))
}

fn ancestor_name(depth: usize) -> String {
    match depth {
        1 => "parent".to_string(),
        2 => "grandparent".to_string(),
        3 => "great-grandparent".to_string(),
        n => format!("{}x-great-grandparent", n - 2),
    }
}

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
    check_plateau_btreemap_key_consistency(graph, errors);
    check_plateau_basis_consistency(graph, errors);
    check_p_i1_i_keys_are_contour_steps(graph, errors);
    check_p_i1_ii_tile_contiguity(graph, errors);
    check_p_i1_iii_run_contains_tile(graph, errors);
    check_p_i2_basis_minimality(graph, errors);
    check_p_i3_basis_disjointness(graph, errors);
    check_p_i4_thatch_one_hop(graph, errors);
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(dead_code)]
pub fn check_p_i3_only<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    check_p_i3_basis_disjointness(graph, errors);
}

fn semi_internal_lineage<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    gnode: GNodeId,
) -> String {
    let mut parts = Vec::new();

    let g = graph.gnodes().get(gnode.index());
    if let Some(parent_id) = g.parent {
        let parent = graph.gnodes().get(parent_id.index());
        if parent.state() == GState::SemiInternal {
            parts.push(format!("SI-child (par=G({}))", parent_id.index()));
        }
    }

    let mut cur = g.parent;
    let mut depth = 1;
    while let Some(anc_id) = cur {
        let anc = graph.gnodes().get(anc_id.index());
        if let Some(anc_parent_id) = anc.parent {
            let anc_parent = graph.gnodes().get(anc_parent_id.index());
            if anc_parent.state() == GState::SemiInternal {
                parts.push(format!(
                    "{} is SI-child (G({}) under G({}))",
                    ancestor_name(depth),
                    anc_id.index(),
                    anc_parent_id.index()
                ));
            }
        }
        cur = anc.parent;
        depth += 1;
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("  [{}]", parts.join("; "))
    }
}

#[allow(dead_code)]
pub fn dump_gtree<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "═══ G-Tree dump (root=GNodeId({}), {} nodes) ═══",
        graph.g_root().index(),
        graph.node_count()
    )
    .unwrap();

    for (idx, g) in graph.gnodes().iter_occupied() {
        let gnode_id = GNodeId::from_index(idx);
        let state = state_label(g.state());
        let depth = gnode_depth_from_interval(g.lo, g.hi, N);
        let parent_str = fmt_optional_gnode(g.parent, "None");
        let left_str = fmt_optional_gnode(g.left, "_");
        let right_str = fmt_optional_gnode(g.right, "_");
        #[cfg(feature = "dynamic-contour-tracking")]
        let basis_str = graph
            .plateau_basis()
            .plateau_key(gnode_id)
            .map_or_else(|| "(not basis)".to_string(), |k| format!("basis={k:?}"));
        #[cfg(not(feature = "dynamic-contour-tracking"))]
        let basis_str = "";
        let lineage = semi_internal_lineage(graph, gnode_id);

        writeln!(out,
            "  G({idx:>3}) {state} [{:>6.1}, {:>6.1})  d={depth}  par={parent_str}  L={left_str} R={right_str}  sum={:>8.1} own={:>8.1}  {basis_str}{lineage}",
            g.lo.to_f64(), g.hi.to_f64(), g.sum.to_f64_approx(), g.own.to_f64_approx(),
        ).unwrap();
    }
    out
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(dead_code)]
pub fn dump_plateaus<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    #[cfg(feature = "dynamic-contour-tracking")]
    let plateaus = &graph.plateaus;
    #[cfg(not(feature = "dynamic-contour-tracking"))]
    let plateaus = graph.build_plateaus();

    writeln!(
        out,
        "═══ Plateau dump ({} plateaus, {} basis members) ═══",
        plateaus.len(),
        graph.plateau_basis().basis_count()
    )
    .unwrap();

    for (key, plateau) in plateaus {
        let next_key = plateaus
            .range(std::ops::RangeFrom {
                start: crate::spatial::plateau::BasisEdge(plateau.end),
            })
            .find(|&(k, _)| *k != *key)
            .map(|(k, _)| k.0.to_f64());
        let tile_end = next_key.map_or_else(|| "∞".to_string(), |v| format!("{v:.1}"));

        writeln!(
            out,
            "  Plateau {key:?}  depth={}  tile=[{:.1}, {tile_end})  span=[{:.1},{:.1})  sum={:.1}",
            plateau.depth,
            key.0.to_f64(),
            plateau.start.to_f64(),
            plateau.end.to_f64(),
            plateau.sum.to_f64_approx(),
        )
        .unwrap();

        let elements = graph.plateau_basis().basis_elements(key);
        let raw_ids: Vec<usize> = elements.iter().map(|g| g.index()).collect();
        writeln!(out, "    basis_elements (raw): {raw_ids:?}").unwrap();

        for &gid in elements {
            if !graph.gnodes().is_occupied(gid.index()) {
                writeln!(
                    out,
                    "    !! DANGLING GNodeId({}) — slot deallocated !!",
                    gid.index()
                )
                .unwrap();
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let state = state_label(g.state());
            let g_depth = gnode_depth_from_interval(g.lo, g.hi, N);
            let contour_depth = match g.state() {
                GState::Terminal | GState::SemiInternal => g_depth,
                GState::Internal => g_depth + 1,
            };
            let parent_str = g
                .parent
                .map_or_else(|| "None".to_string(), |p| format!("G({})", p.index()));
            let left_str = g
                .left
                .map_or_else(|| "_".to_string(), |l| format!("G({})", l.index()));
            let right_str = g
                .right
                .map_or_else(|| "_".to_string(), |r| format!("G({})", r.index()));
            let lineage = semi_internal_lineage(graph, gid);

            writeln!(out,
                "    G({:>3}) {state} [{:>6.1}, {:>6.1})  d={g_depth} contour_d={contour_depth}  par={parent_str} L={left_str} R={right_str}  sum={:.1}{lineage}",
                gid.index(), g.lo.to_f64(), g.hi.to_f64(), g.sum.to_f64_approx(),
            ).unwrap();
        }
    }

    writeln!(
        out,
        "  ─── Back map ({} entries) ───",
        graph.plateau_basis().back_map().len()
    )
    .unwrap();
    let mut back_entries: Vec<_> = graph
        .plateau_basis()
        .back_map()
        .iter()
        .map(|(&gid, &key)| (gid.index(), key))
        .collect();
    back_entries.sort_by_key(|(idx, _)| *idx);
    for (idx, key) in &back_entries {
        writeln!(out, "    G({idx}) → {key:?}").unwrap();
    }

    let mut p_i3_errors = Vec::new();
    check_p_i3_basis_disjointness(graph, &mut p_i3_errors);
    if !p_i3_errors.is_empty() {
        writeln!(out, "  ─── P-I3 violations ───").unwrap();
        for e in &p_i3_errors {
            writeln!(out, "    !! {e}").unwrap();
        }
    }

    let mut p_i4_errors = Vec::new();
    check_p_i4_thatch_one_hop(graph, &mut p_i4_errors);
    if !p_i4_errors.is_empty() {
        writeln!(out, "  ─── P-I4 violations ───").unwrap();
        for e in &p_i4_errors {
            writeln!(out, "    !! {e}").unwrap();
        }
    }

    out
}

#[allow(dead_code)]
pub fn check_all_invariants<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    check_g_i1_summation(graph, &mut errors);
    check_g_i2_variable_fanout(graph, &mut errors);
    check_g_i4_entry_consistency(graph, &mut errors);
    check_v_i1_structural_sum(graph, &mut errors);
    check_v_i2_branching_factor(graph, &mut errors);
    check_v_i3_max_uncle(graph, &mut errors);
    check_v_i5_entry_leaf(graph, &mut errors);
    check_v_i6_exposed_flag(graph, &mut errors);
    check_v_i6b_evictable_flag(graph, &mut errors);
    check_v_i7_structural_flag(graph, &mut errors);
    check_clean_accounting(graph, &mut errors);
    check_parent_link_consistency(graph, &mut errors);
    check_v_root_consistency(graph, &mut errors);
    check_node_count_consistency(graph, &mut errors);
    check_terminal_count_consistency(graph, &mut errors);
    check_depth_gate_invariants(graph, &mut errors);
    check_hard_budget(graph, &mut errors);
    #[cfg(feature = "dynamic-contour-tracking")]
    {
        check_plateau_btreemap_key_consistency(graph, &mut errors);
        check_plateau_basis_consistency(graph, &mut errors);
        check_plateau_sum_consistency(graph, &mut errors);
        check_plateau_depth_consistency(graph, &mut errors);
        check_p_i1_i_keys_are_contour_steps(graph, &mut errors);
        check_p_i1_ii_tile_contiguity(graph, &mut errors);
        check_p_i1_iii_run_contains_tile(graph, &mut errors);
        check_p_i2_basis_minimality(graph, &mut errors);
        check_p_i3_basis_disjointness(graph, &mut errors);
        check_p_i4_thatch_one_hop(graph, &mut errors);
        check_p_i5_thatch_depth(graph, &mut errors);
    }

    errors
}

#[allow(clippy::float_cmp)]
fn check_g_i1_summation<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gnodes().iter_occupied() {
        let left_sum = g
            .left
            .map_or(0.0, |l| graph.gnodes().get(l.index()).sum.to_f64_approx());
        let right_sum = g
            .right
            .map_or(0.0, |r| graph.gnodes().get(r.index()).sum.to_f64_approx());
        let expected = g.own.to_f64_approx() + left_sum + right_sum;
        let actual = g.sum.to_f64_approx();
        if expected != actual && (expected - actual).abs() > 1e-9 {
            errors.push(format!(
                "G-I1 violated at G-node {idx}: expected sum={expected}, actual sum={actual} \
                 (own={}, left_sum={left_sum}, right_sum={right_sum})",
                g.own.to_f64_approx()
            ));
        }
    }
}

fn check_g_i2_variable_fanout<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, g) in graph.gnodes().iter_occupied() {
        let count = usize::from(g.left.is_some()) + usize::from(g.right.is_some());
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
    for (idx, g) in graph.gnodes().iter_occupied() {
        if let Some(v_id) = g.entry {
            if !graph.vnodes().is_occupied(v_id.index()) {
                errors.push(format!(
                    "G-I4 violated at G-node {idx}: entry V-node {} is not occupied",
                    v_id.index()
                ));
                continue;
            }
            let v = graph.vnodes().get(v_id.index());

            let g_own = g.own.to_f64_approx();
            let v_int = v.intensity.to_f64_approx();
            #[allow(clippy::float_cmp)]
            if g_own != v_int && (g_own - v_int).abs() > 1e-9 {
                errors.push(format!(
                    "G-I4 violated at G-node {idx}: g.own={g_own}, entry.intensity={v_int}"
                ));
            }

            match &v.kind {
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
fn check_v_i1_structural_sum<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (idx, v) in graph.vnodes().iter_occupied() {
        if let VKind::Structural { children, .. } = &v.kind {
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
                let actual_int = graph.vnodes().get(child_id.index()).intensity;
                if (cached_int.to_f64_approx() != actual_int.to_f64_approx())
                    && (cached_int.to_f64_approx() - actual_int.to_f64_approx()).abs() > 1e-9
                {
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

            let node_int = v.intensity.to_f64_approx();
            if node_int != sum && (node_int - sum).abs() > 1e-9 {
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
        if let VKind::Structural { children, .. } = &v.kind {
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
            let int = v.intensity.to_f64_approx();
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
        if let VKind::Structural { children, .. } = &v.kind {
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
        } = &v.kind
        {
            if !graph.gnodes().is_occupied(gnode.index()) {
                errors.push(format!(
                    "V-I6 violated at V-node {idx}: backing G-node {} is not occupied",
                    gnode.index()
                ));
                continue;
            }
            let g = graph.gnodes().get(gnode.index());
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
        } = &v.kind
        {
            if !graph.gnodes().is_occupied(gnode.index()) {
                continue;
            }
            let g = graph.gnodes().get(gnode.index());
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
        } = &v.kind
        {
            let expected = (0..children.len()).any(|i| {
                let (child_id, _) = children.get(i);
                if !graph.vnodes().is_occupied(child_id.index()) {
                    return false;
                }
                let child = graph.vnodes().get(child_id.index());
                match &child.kind {
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
        if matches!(v.kind, VKind::Entry { .. }) {
            total_v += v.intensity.to_f64_approx();
        }
    }
    let g_root_sum = graph
        .gnodes()
        .get(graph.g_root().index())
        .sum
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
    for (idx, g) in graph.gnodes().iter_occupied() {
        let g_id = GNodeId::from_index(idx);
        if let Some(left) = g.left {
            if graph.gnodes().is_occupied(left.index()) {
                let child_parent = graph.gnodes().get(left.index()).parent;
                if child_parent != Some(g_id) {
                    errors.push(format!(
                        "G-parent link: G-node {idx}'s left child {}'s parent is {:?}, expected {g_id:?}",
                        left.index(),
                        child_parent
                    ));
                }
            }
        }
        if let Some(right) = g.right {
            if graph.gnodes().is_occupied(right.index()) {
                let child_parent = graph.gnodes().get(right.index()).parent;
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

    for (idx, v) in graph.vnodes().iter_occupied() {
        let v_id = VNodeId::from_index(idx);
        if let VKind::Structural { children, .. } = &v.kind {
            for i in 0..children.len() {
                let (child_id, _) = children.get(i);
                if graph.vnodes().is_occupied(child_id.index()) {
                    let child_parent = graph.vnodes().get(child_id.index()).parent;
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

        if let Some(p_id) = v.parent {
            if graph.vnodes().is_occupied(p_id.index()) {
                let parent = graph.vnodes().get(p_id.index());
                if let VKind::Structural { children, .. } = &parent.kind {
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
        if root.parent.is_some() {
            errors.push(format!(
                "V-root consistency: v_root {} has parent {:?}, expected None",
                root_id.index(),
                root.parent
            ));
        }
    }
}

fn check_node_count_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let actual = graph.gnodes().count();
    let expected = graph.node_count();
    if actual != expected {
        errors.push(format!(
            "Node count: graph.node_count()={expected}, arena count={actual}"
        ));
    }
}

fn check_terminal_count_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    #[allow(clippy::cast_possible_truncation)]
    let actual = graph
        .gnodes()
        .iter_occupied()
        .filter(|(_, g)| g.is_terminal())
        .count() as u32;
    let expected = graph.terminal_count();
    if actual != expected {
        errors.push(format!(
            "Terminal count: graph.terminal_count()={expected}, arena walk={actual}"
        ));
    }
}

fn check_hard_budget<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    if let Some(budget) = graph.config().budget {
        if graph.node_count() as usize > budget {
            errors.push(format!(
                "Hard budget violated (ADR-M-018): node_count ({}) > budget ({})",
                graph.node_count(),
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
    let buffer = graph.depth_buffer();

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

#[cfg(feature = "dynamic-contour-tracking")]
fn check_plateau_btreemap_key_consistency<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (&key, plateau) in &graph.plateaus {
        if plateau.basis_edge != key {
            errors.push(format!(
                "Plateau key consistency: BTreeMap key {key:?} != plateau.basis_edge {:?}",
                plateau.basis_edge
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_plateau_basis_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();

    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                errors.push(format!(
                    "Plateau basis: basis element {gid:?} in plateau {key:?} is not a live arena slot"
                ));
                continue;
            }

            match pb.plateau_key(gid) {
                Some(back_key) if back_key == key => {}
                Some(back_key) => {
                    errors.push(format!(
                        "Plateau basis forward→back: {gid:?} in forward[{key:?}] but back[{gid:?}] = {back_key:?}"
                    ));
                }
                None => {
                    errors.push(format!(
                        "Plateau basis forward→back: {gid:?} in forward[{key:?}] but not in back map"
                    ));
                }
            }
        }
    }

    for (&gid, &key) in pb.back_map() {
        let elements = pb.basis_elements(&key);
        if !elements.contains(&gid) {
            errors.push(format!(
                "Plateau basis back→forward: back[{gid:?}] = {key:?} but {gid:?} not in forward[{key:?}]"
            ));
        }
    }

    let btree_keys: Vec<_> = graph.plateaus.keys().copied().collect();
    let basis_keys: Vec<_> = pb.iter().map(|(&k, _)| k).collect();
    if btree_keys != basis_keys {
        errors.push(format!(
            "Plateau basis: plateaus.keys() ({} entries) != plateau_basis.forward.keys() ({} entries)",
            btree_keys.len(),
            basis_keys.len()
        ));
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(clippy::float_cmp)]
fn check_plateau_sum_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, plateau) in &graph.plateaus {
        let expected: f64 = pb
            .basis_elements(&key)
            .iter()
            .filter(|&&gid| graph.gnodes().is_occupied(gid.index()))
            .map(|&gid| graph.gnodes().get(gid.index()).sum.to_f64_approx())
            .sum();
        let actual = plateau.sum.to_f64_approx();
        if expected != actual && (expected - actual).abs() > 1e-9 {
            errors.push(format!(
                "Plateau sum: key {key:?}: expected sum={expected}, actual sum={actual}"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_plateau_depth_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, plateau) in &graph.plateaus {
        for &gid in pb.basis_elements(&key) {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let g_depth = gnode_depth_from_interval(g.lo, g.hi, N);
            let expected_depth = match g.state() {
                GState::Terminal | GState::SemiInternal => g_depth,
                GState::Internal => {
                    let Some(d) = crate::graph::uniform_contour_depth_of(graph.gnodes(), gid, N)
                    else {
                        errors.push(format!(
                            "Plateau depth: key {key:?}, basis element {gid:?} (Internal): \
                             uniform_contour_depth_of returned None — internal basis \
                             element has non-uniform contour depth",
                        ));
                        continue;
                    };
                    d
                }
            };
            if expected_depth != plateau.depth {
                errors.push(format!(
                    "Plateau depth: key {key:?}, basis element {gid:?} ({:?}): \
                     element contributes depth {expected_depth}, plateau.depth={}",
                    g.state(),
                    plateau.depth,
                ));
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn tile_of<C: Coordinate, V: Accumulator>(g: &crate::nodes::gnode::GNode<C, V>) -> (C, C) {
    match g.state() {
        GState::Terminal | GState::Internal => (g.lo, g.hi),
        GState::SemiInternal => g
            .uncovered_range()
            .expect("semi-internal must have uncovered range"),
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn contour_steps<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<(C, u32)> {
    use crate::tree::gtree::gnode_depth_from_interval;

    let mut cells: Vec<(C, u32)> = Vec::new();
    let mut stack = vec![graph.g_root()];
    while let Some(gid) = stack.pop() {
        let g = graph.gnodes().get(gid.index());
        match g.state() {
            GState::Terminal => {
                let d = gnode_depth_from_interval(g.lo, g.hi, N);
                cells.push((g.lo, d));
            }
            GState::SemiInternal => {
                let (ulo, _uhi) = g.uncovered_range().unwrap();
                let d = gnode_depth_from_interval(g.lo, g.hi, N);
                cells.push((ulo, d));

                if let Some(l) = g.left {
                    stack.push(l);
                }
                if let Some(r) = g.right {
                    stack.push(r);
                }
            }
            GState::Internal => {
                if let Some(l) = g.left {
                    stack.push(l);
                }
                if let Some(r) = g.right {
                    stack.push(r);
                }
            }
        }
    }
    cells.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut steps: Vec<(C, u32)> = Vec::new();
    for &(lo, depth) in &cells {
        if steps.is_empty() || steps.last().unwrap().1 != depth {
            steps.push((lo, depth));
        }
    }
    steps
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i1_i_keys_are_contour_steps<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    if plateaus.is_empty() {
        errors.push("P-I1(i): plateaus BTreeMap is empty (must have at least 1 plateau)".into());
        return;
    }

    let steps = contour_steps(graph);
    let btree_keys: Vec<C> = plateaus.keys().map(|k| k.0).collect();
    let step_coords: Vec<C> = steps.iter().map(|&(c, _)| c).collect();

    if btree_keys.len() != step_coords.len() {
        errors.push(format!(
            "P-I1(i): plateau count ({}) != contour step count ({})",
            btree_keys.len(),
            step_coords.len()
        ));
    }

    for (i, (&(coord, depth), btree_coord)) in steps.iter().zip(btree_keys.iter()).enumerate() {
        if coord.total_cmp(btree_coord) != std::cmp::Ordering::Equal {
            errors.push(format!(
                "P-I1(i): step {i}: contour step at {coord:?}, BTreeMap key at {btree_coord:?}"
            ));
        }

        if let Some(plateau) = plateaus.get(&BasisEdge(coord)) {
            if plateau.depth != depth {
                errors.push(format!(
                    "P-I1(i): step {i} at {coord:?}: contour depth={depth}, plateau.depth={}",
                    plateau.depth
                ));
            }
        }
    }

    let depths: Vec<u32> = plateaus.values().map(|p| p.depth).collect();
    for w in depths.windows(2) {
        if w[0] == w[1] {
            let keys: Vec<_> = plateaus.keys().collect();
            let idx = depths.windows(2).position(|d| d[0] == d[1]).unwrap();
            errors.push(format!(
                "P-I1(i) maximality: consecutive plateaus {:?} and {:?} both have depth {}",
                keys[idx],
                keys[idx + 1],
                w[0]
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i1_ii_tile_contiguity<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    let pb = graph.plateau_basis();
    let keys: Vec<BasisEdge<C>> = plateaus.keys().copied().collect();
    let domain_max = C::domain_max(N);

    for (i, &key) in keys.iter().enumerate() {
        let next_start = if i + 1 < keys.len() {
            keys[i + 1].0
        } else {
            domain_max
        };

        let elements = pb.basis_elements(&key);
        if elements.is_empty() {
            errors.push(format!("P-I1(ii): plateau {key:?} has no basis elements"));
            continue;
        }

        let mut tiles: Vec<(f64, f64)> = Vec::new();
        for &gid in elements {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let (tlo, thi) = tile_of(g);
            tiles.push((tlo.to_f64(), thi.to_f64()));
        }
        tiles.sort_by(|a, b| a.0.total_cmp(&b.0));

        let expected_lo = key.0.to_f64();
        let expected_hi = next_start.to_f64();

        if tiles.is_empty() {
            errors.push(format!("P-I1(ii): plateau {key:?}: no live basis tiles"));
            continue;
        }

        if (tiles[0].0 - expected_lo).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(ii): plateau {key:?}: tile union starts at {}, expected {expected_lo}",
                tiles[0].0
            ));
        }

        let last_hi = tiles.last().unwrap().1;
        if (last_hi - expected_hi).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(ii): plateau {key:?}: tile union ends at {last_hi}, expected {expected_hi}"
            ));
        }

        for w in tiles.windows(2) {
            if w[1].0 - w[0].1 > 1e-12 {
                errors.push(format!(
                    "P-I1(ii): plateau {key:?}: gap in tiles between {} and {}",
                    w[0].1, w[1].0
                ));
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i1_iii_run_contains_tile<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    let pb = graph.plateau_basis();
    let keys: Vec<BasisEdge<C>> = plateaus.keys().copied().collect();
    let domain_max = C::domain_max(N);

    for (i, &key) in keys.iter().enumerate() {
        let plateau = &plateaus[&key];
        let next_start = if i + 1 < keys.len() {
            keys[i + 1].0
        } else {
            domain_max
        };

        let elements = pb.basis_elements(&key);
        if elements.is_empty() {
            continue;
        }

        let mut min_lo = f64::INFINITY;
        let mut max_hi = f64::NEG_INFINITY;
        for &gid in elements {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let lo = g.lo.to_f64();
            let hi = g.hi.to_f64();
            if lo < min_lo {
                min_lo = lo;
            }
            if hi > max_hi {
                max_hi = hi;
            }
        }

        let p_start = plateau.start.to_f64();
        let p_end = plateau.end.to_f64();
        if (p_start - min_lo).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: start={p_start}, expected min(basis.lo)={min_lo}"
            ));
        }
        if (p_end - max_hi).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: end={p_end}, expected max(basis.hi)={max_hi}"
            ));
        }

        let tile_lo = key.0.to_f64();
        let tile_hi = next_start.to_f64();
        if p_start - tile_lo > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: run start {p_start} > tile start {tile_lo}"
            ));
        }
        if tile_hi - p_end > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: run end {p_end} < tile end {tile_hi}"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i2_basis_minimality<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, gnodes_list) in pb.iter() {
        let Some(plateau) = graph.plateaus.get(&key) else {
            continue;
        };
        let expected_depth = plateau.depth;

        for &gid in gnodes_list {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());

            if g.state() == GState::SemiInternal {
                continue;
            }

            if let Some(parent_id) = g.parent {
                if let Some(parent_depth) = uniform_contour_depth_of(graph.gnodes(), parent_id, N) {
                    if parent_depth == expected_depth {
                        errors.push(format!(
                            "P-I2 minimality: basis element G({}) in plateau {key:?} \
                             has parent G({}) with uniform contour depth {parent_depth} \
                             == plateau depth {expected_depth} — parent should be the \
                             basis element instead",
                            gid.index(),
                            parent_id.index(),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i3_basis_disjointness<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    let mut tiles: Vec<(f64, f64, BasisEdge<C>)> = Vec::new();
    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let (tlo, thi) = tile_of(g);
            tiles.push((tlo.to_f64(), thi.to_f64(), key));
        }
    }
    tiles.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));

    for w in tiles.windows(2) {
        let (lo1, hi1, k1) = &w[0];
        let (lo2, _hi2, k2) = &w[1];
        if k1 != k2 && *hi1 > *lo2 + 1e-12 {
            errors.push(format!(
                "P-I3 tile disjointness: tile in plateau {k1:?} [{lo1}, {hi1}) \
                 overlaps tile in plateau {k2:?} [{lo2}, ..)"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i4_thatch_one_hop<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            if g.state() != GState::SemiInternal {
                continue;
            }

            let child = g.left.or(g.right);
            let Some(child_id) = child else {
                errors.push(format!(
                    "P-I4: semi-internal {gid:?} in plateau {key:?} has no children"
                ));
                continue;
            };

            let child_g = graph.gnodes().get(child_id.index());
            let child_lo = child_g.lo;

            let child_plateau_key = graph
                .plateaus
                .range(..=BasisEdge(child_lo))
                .next_back()
                .map(|(&k, _)| k);

            match child_plateau_key {
                Some(ck) if ck == key => {
                    errors.push(format!(
                        "P-I4 thatch one-hop: semi-internal {gid:?} in plateau {key:?} \
                         thatches child {child_id:?}, but child's plateau key {ck:?} == parent's"
                    ));
                }
                None => {
                    errors.push(format!(
                        "P-I4 thatch one-hop: semi-internal {gid:?} in plateau {key:?}: \
                         no plateau found covering child {child_id:?} at lo={child_lo:?}"
                    ));
                }
                Some(_) => {}
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn check_p_i5_thatch_depth<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();

    let mut samples: Vec<C> = Vec::new();
    for (_, gnodes) in pb.iter() {
        for &gid in gnodes {
            if graph.gnodes().is_occupied(gid.index()) {
                samples.push(graph.gnodes().get(gid.index()).lo);
            }
        }
    }
    samples.sort_by(Coordinate::total_cmp);
    samples.dedup_by(|a, b| a.total_cmp(b) == std::cmp::Ordering::Equal);

    for x in &samples {
        let mut thatch_count = 0u32;
        for (&_key, gnodes) in pb.iter() {
            let covers = gnodes.iter().any(|&gid| {
                if !graph.gnodes().is_occupied(gid.index()) {
                    return false;
                }
                let g = graph.gnodes().get(gid.index());
                g.lo.total_cmp(x) != std::cmp::Ordering::Greater
                    && x.total_cmp(&g.hi) == std::cmp::Ordering::Less
            });
            if covers {
                thatch_count += 1;
            }
        }

        let d_geo = route_to_depth(graph, *x);

        if thatch_count > d_geo + 1 {
            errors.push(format!(
                "P-I5 thatch depth: at x={x:?}, thatch_depth={thatch_count} > d_geo={d_geo} + 1"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn route_to_depth<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    x: C,
) -> u32 {
    let mut cur = graph.g_root();
    for _ in 0..=N + 1 {
        let g = graph.gnodes().get(cur.index());
        if g.is_terminal() {
            return gnode_depth_from_interval(g.lo, g.hi, N);
        }
        let mid = C::midpoint(g.lo, g.hi);
        let next = if x.total_cmp(&mid) == std::cmp::Ordering::Less {
            g.left
        } else {
            g.right
        };
        match next {
            Some(child) => cur = child,
            None => return gnode_depth_from_interval(g.lo, g.hi, N),
        }
    }
    let g = graph.gnodes().get(cur.index());
    gnode_depth_from_interval(g.lo, g.hi, N)
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::invariants::{assert_invariants, check_all_invariants, dump_gtree};
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

    fn fresh() -> G {
        GvGraph::new(make_config())
    }

    // ── assert_invariants / check_all_invariants ──────────────────────
    mod assert_invariants_fn {
        use super::*;

        #[test]
        fn does_not_panic_for_fresh_graph() {
            assert_invariants(&fresh());
        }

        #[test]
        fn does_not_panic_after_single_observation() {
            let mut g = fresh();
            g.observe(64u8, 2u32);
            assert_invariants(&g);
        }

        #[test]
        fn does_not_panic_after_bootstrap_split() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            assert_invariants(&g);
        }

        #[test]
        fn does_not_panic_after_multiple_observations() {
            let mut g = fresh();
            for coord in [0u8, 64, 128, 192, 32, 96, 160, 224] {
                g.observe(coord, 3u32);
            }
            assert_invariants(&g);
        }
    }

    mod check_all_invariants_fn {
        use super::*;

        #[test]
        fn returns_empty_errors_for_fresh_graph() {
            let errors = check_all_invariants(&fresh());
            assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        }

        #[test]
        fn returns_empty_errors_after_observations() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            g.observe(32u8, 3u32);
            let errors = check_all_invariants(&g);
            assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        }
    }

    // ── dump_gtree ────────────────────────────────────────────────────
    mod dump_gtree_fn {
        use super::*;

        #[test]
        fn returns_non_empty_string_for_fresh_graph() {
            let g = fresh();
            let s = dump_gtree(&g);
            assert!(!s.is_empty());
        }

        #[test]
        fn output_contains_g_tree_header() {
            let g = fresh();
            let s = dump_gtree(&g);
            assert!(s.contains("G-Tree dump"), "header not found in: {s}");
        }
    }

    // ── dump_plateaus ─────────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod dump_plateaus_fn {
        use super::*;
        use crate::diagnostics::invariants::dump_plateaus;

        #[test]
        fn returns_non_empty_string_for_fresh_graph() {
            let g = fresh();
            let s = dump_plateaus(&g);
            assert!(!s.is_empty());
        }

        #[test]
        fn output_contains_plateau_dump_header_after_bootstrap() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            let s = dump_plateaus(&g);
            assert!(s.contains("Plateau dump"), "header not found in: {s}");
        }

        #[test]
        fn works_after_multiple_splits() {
            let mut g = fresh();
            for coord in [32u8, 96, 160, 224] {
                g.observe(coord, 3u32);
            }
            let s = dump_plateaus(&g);
            assert!(!s.is_empty());
        }
    }

    // ── check_plateau_only ────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod check_plateau_only_fn {
        use super::*;
        use crate::diagnostics::invariants::check_plateau_only;

        #[test]
        fn no_errors_for_fresh_graph() {
            let g = fresh();
            let mut errors = Vec::new();
            check_plateau_only(&g, &mut errors);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }

        #[test]
        fn no_errors_after_bootstrap_split() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            let mut errors = Vec::new();
            check_plateau_only(&g, &mut errors);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }

        #[test]
        fn no_errors_after_multiple_splits() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            g.observe(64u8, 3u32);
            let mut errors = Vec::new();
            check_plateau_only(&g, &mut errors);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }
    }

    // ── check_p_i3_only ───────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod check_p_i3_only_fn {
        use super::*;
        use crate::diagnostics::invariants::check_p_i3_only;

        #[test]
        fn no_errors_for_fresh_graph() {
            let g = fresh();
            let mut errors = Vec::new();
            check_p_i3_only(&g, &mut errors);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }

        #[test]
        fn no_errors_after_multiple_splits() {
            let mut g = fresh();
            g.observe(64u8, 3u32);
            g.observe(64u8, 3u32);
            let mut errors = Vec::new();
            check_p_i3_only(&g, &mut errors);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }
    }

    // ── ancestor_name (private fn) ────────────────────────────────────
    mod ancestor_name_fn {
        #[test]
        fn depth_1_is_parent() {
            assert_eq!(super::super::ancestor_name(1), "parent");
        }

        #[test]
        fn depth_2_is_grandparent() {
            assert_eq!(super::super::ancestor_name(2), "grandparent");
        }

        #[test]
        fn depth_3_is_great_grandparent() {
            assert_eq!(super::super::ancestor_name(3), "great-grandparent");
        }

        #[test]
        fn depth_4_uses_format_path() {
            // n=4: "2x-great-grandparent"
            let s = super::super::ancestor_name(4);
            assert!(s.contains("great-grandparent"), "got: {s}");
        }
    }
}

// ── DOT / Graphviz snapshot helpers ─────────────────────────────────────────

/// Render the **G-Tree** (binary range-split tree) as a Graphviz DOT string.
///
/// Each node shows:
/// - node id, coordinate range `[lo, hi)`
/// - geometric depth (`d`), node state (`T`=Terminal, `I`=Internal,
///   `S`=SemiInternal)
/// - accumulated `own` value and subtree `sum`
/// - whether a V-Tree Entry is currently attached
///
/// Nodes are colour-coded by state:
/// - green  — Terminal with a V-tree entry
/// - grey   — Terminal without a V-tree entry (orphaned leaf)
/// - yellow — Internal (both children present)
/// - orange — SemiInternal (one child present)
#[allow(dead_code)]
pub fn dump_gtree_dot<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    label: &str,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "digraph gtree {{").unwrap();
    writeln!(out, "  label={label:?};").unwrap();
    writeln!(out, "  rankdir=TB;").unwrap();
    writeln!(
        out,
        "  node [shape=box, fontname=\"Courier New\", fontsize=11];"
    )
    .unwrap();
    writeln!(out).unwrap();

    // BFS from the root so the node ordering in the file is breadth-first.
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(graph.g_root());

    while let Some(gid) = queue.pop_front() {
        let g = graph.gnodes().get(gid.index());
        let idx = gid.index();
        let depth = gnode_depth_from_interval(g.lo, g.hi, N);
        let state = state_label(g.state());
        let has_entry = g.entry.is_some();

        let fillcolor = match g.state() {
            GState::Terminal if has_entry => "#c8e6c9", // green
            GState::Terminal => "#e0e0e0",              // grey
            GState::Internal => "#fff9c4",              // yellow
            GState::SemiInternal => "#ffe0b2",          // orange
        };

        let entry_note = if has_entry {
            format!("VEntry({})", g.entry.unwrap().index())
        } else {
            "no VEntry".to_string()
        };

        writeln!(
            out,
            "  G{idx} [label=\"G{idx}  [{:.0}, {:.0})\\nd={depth}  {state}  {entry_note}\\nown={:.0}  sum={:.0}\", \
             style=filled, fillcolor=\"{fillcolor}\"];",
            g.lo.to_f64(),
            g.hi.to_f64(),
            g.own.to_f64_approx(),
            g.sum.to_f64_approx(),
        )
        .unwrap();

        if let Some(left) = g.left {
            queue.push_back(left);
        }
        if let Some(right) = g.right {
            queue.push_back(right);
        }
    }

    writeln!(out).unwrap();

    // Edges (separate pass so all nodes are declared before edges).
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(graph.g_root());
    while let Some(gid) = queue.pop_front() {
        let g = graph.gnodes().get(gid.index());
        let idx = gid.index();
        if let Some(left) = g.left {
            writeln!(out, "  G{idx} -> G{} [label=\"L\"];", left.index()).unwrap();
            queue.push_back(left);
        }
        if let Some(right) = g.right {
            writeln!(out, "  G{idx} -> G{} [label=\"R\"];", right.index()).unwrap();
            queue.push_back(right);
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

/// Render the **V-Tree** (virtual intensity tree) as a Graphviz DOT string.
///
/// Two kinds of V-nodes are distinguished:
/// - **Entry** (blue box) — leaf in the V-Tree, backed by a real G-node. Shows
///   the linked G-node id, its coordinate range, geometric depth, state,
///   the accumulated intensity, and the `exposed` / `evictable` flags.
/// - **Structural** (purple ellipse) — internal routing node that aggregates
///   child intensities.  Shows the aggregated intensity and the
///   `has_evictable` flag.
#[allow(dead_code)]
pub fn dump_vtree_dot<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    label: &str,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let Some(v_root) = graph.v_root() else {
        writeln!(out, "digraph vtree {{").unwrap();
        writeln!(out, "  label={label:?};").unwrap();
        writeln!(
            out,
            "  empty [label=\"(empty — no observations yet)\", shape=plaintext];"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        return out;
    };

    writeln!(out, "digraph vtree {{").unwrap();
    writeln!(out, "  label={label:?};").unwrap();
    writeln!(out, "  rankdir=TB;").unwrap();
    writeln!(out, "  node [fontname=\"Courier New\", fontsize=11];").unwrap();
    writeln!(out).unwrap();

    // Declare nodes (BFS).
    let mut queue: std::collections::VecDeque<VNodeId> = std::collections::VecDeque::new();
    queue.push_back(v_root);

    while let Some(vid) = queue.pop_front() {
        let vnode = graph.vnodes().get(vid.index());
        let idx = vid.index();

        match &vnode.kind {
            VKind::Entry {
                gnode,
                is_exposed,
                is_evictable,
            } => {
                let g = graph.gnodes().get(gnode.index());
                let g_depth = gnode_depth_from_interval(g.lo, g.hi, N);
                let g_state = state_label(g.state());
                writeln!(
                    out,
                    "  V{idx} [shape=box, style=filled, fillcolor=\"#bbdefb\", \
                     label=\"V{idx} Entry\\nG{}  [{:.0},{:.0}) d={g_depth} {g_state}\\n\
                     intensity={:.0}  exposed={is_exposed}  evict={is_evictable}\"];",
                    gnode.index(),
                    g.lo.to_f64(),
                    g.hi.to_f64(),
                    vnode.intensity.to_f64_approx(),
                )
                .unwrap();
            }
            VKind::Structural {
                children,
                has_evictable,
            } => {
                writeln!(
                    out,
                    "  V{idx} [shape=ellipse, style=filled, fillcolor=\"#e1bee7\", \
                     label=\"V{idx} Struct\\nintensity={:.0}  has_evict={has_evictable}\"];",
                    vnode.intensity.to_f64_approx(),
                )
                .unwrap();
                for i in 0..children.len() {
                    let (child_id, _) = children.get(i);
                    queue.push_back(child_id);
                }
            }
        }
    }

    writeln!(out).unwrap();

    // Edges (separate BFS pass).
    let mut queue: std::collections::VecDeque<VNodeId> = std::collections::VecDeque::new();
    queue.push_back(v_root);

    while let Some(vid) = queue.pop_front() {
        let vnode = graph.vnodes().get(vid.index());
        let idx = vid.index();

        if let VKind::Structural { children, .. } = &vnode.kind {
            for i in 0..children.len() {
                let (child_id, child_intensity) = children.get(i);
                writeln!(
                    out,
                    "  V{idx} -> V{} [label=\"i={:.0}\"];",
                    child_id.index(),
                    child_intensity.to_f64_approx(),
                )
                .unwrap();
                queue.push_back(child_id);
            }
        }
    }

    writeln!(out, "}}").unwrap();
    out
}
