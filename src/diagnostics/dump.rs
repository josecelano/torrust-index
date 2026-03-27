use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::nodes::gnode::GState;
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::gtree::GTree;

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

fn semi_internal_lineage<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    gnode: GNodeId,
) -> String {
    let mut parts = Vec::new();

    let g = graph.gtree.nodes.get(gnode.index());
    if let Some(parent_id) = g.parent() {
        let parent = graph.gtree.nodes.get(parent_id.index());
        if parent.state() == GState::SemiInternal {
            parts.push(format!("SI-child (par=G({}))", parent_id.index()));
        }
    }

    let mut cur = g.parent();
    let mut depth = 1;
    while let Some(anc_id) = cur {
        let anc = graph.gtree.nodes.get(anc_id.index());
        if let Some(anc_parent_id) = anc.parent() {
            let anc_parent = graph.gtree.nodes.get(anc_parent_id.index());
            if anc_parent.state() == GState::SemiInternal {
                parts.push(format!(
                    "{} is SI-child (G({}) under G({}))",
                    ancestor_name(depth),
                    anc_id.index(),
                    anc_parent_id.index()
                ));
            }
        }
        cur = anc.parent();
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
        graph.gtree.root.index(),
        graph.gtree.node_count
    )
    .unwrap();

    for (idx, g) in graph.gtree.nodes.iter_occupied() {
        let gnode_id = GNodeId::from_index(idx);
        let state = state_label(g.state());
        let depth = GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi());
        let parent_str = fmt_optional_gnode(g.parent(), "None");
        let left_str = fmt_optional_gnode(g.left(), "_");
        let right_str = fmt_optional_gnode(g.right(), "_");
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
            g.lo().to_f64(), g.hi().to_f64(), g.sum().to_f64_approx(), g.own().to_f64_approx(),
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
            if !graph.gtree.nodes.is_occupied(gid.index()) {
                writeln!(
                    out,
                    "    !! DANGLING GNodeId({}) — slot deallocated !!",
                    gid.index()
                )
                .unwrap();
                continue;
            }
            let g = graph.gtree.nodes.get(gid.index());
            let state = state_label(g.state());
            let g_depth = GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi());
            let contour_depth = match g.state() {
                GState::Terminal | GState::SemiInternal => g_depth,
                GState::Internal => g_depth + 1,
            };
            let parent_str = g
                .parent()
                .map_or_else(|| "None".to_string(), |p| format!("G({})", p.index()));
            let left_str = g
                .left()
                .map_or_else(|| "_".to_string(), |l| format!("G({})", l.index()));
            let right_str = g
                .right()
                .map_or_else(|| "_".to_string(), |r| format!("G({})", r.index()));
            let lineage = semi_internal_lineage(graph, gid);

            writeln!(out,
                "    G({:>3}) {state} [{:>6.1}, {:>6.1})  d={g_depth} contour_d={contour_depth}  par={parent_str} L={left_str} R={right_str}  sum={:.1}{lineage}",
                gid.index(), g.lo().to_f64(), g.hi().to_f64(), g.sum().to_f64_approx(),
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
    crate::diagnostics::plateau_invariants::check_p_i3_basis_disjointness(graph, &mut p_i3_errors);
    if !p_i3_errors.is_empty() {
        writeln!(out, "  ─── P-I3 violations ───").unwrap();
        for e in &p_i3_errors {
            writeln!(out, "    !! {e}").unwrap();
        }
    }

    let mut p_i4_errors = Vec::new();
    crate::diagnostics::plateau_invariants::check_p_i4_thatch_one_hop(graph, &mut p_i4_errors);
    if !p_i4_errors.is_empty() {
        writeln!(out, "  ─── P-I4 violations ───").unwrap();
        for e in &p_i4_errors {
            writeln!(out, "    !! {e}").unwrap();
        }
    }

    out
}

#[cfg(test)]
mod tests {
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
