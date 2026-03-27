use crate::graph::GvGraph;
use crate::handle::VNodeId;
use crate::nodes::gnode::GState;
use crate::nodes::vnode::VKind;
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::tree::gtree::gnode_depth_from_interval;

const fn state_label(s: GState) -> &'static str {
    match s {
        GState::Terminal => "T",
        GState::SemiInternal => "S",
        GState::Internal => "I",
    }
}

/// Render the **G-Tree** (binary range-split tree) as a Graphviz DOT string.
///
/// Each node shows:
/// - node id, coordinate range `[lo, hi)`
/// - geometric depth (`d`), node state (`T`=Terminal, `I`=Internal,
///   `S`=`SemiInternal`)
/// - accumulated `own` value and subtree `sum`
/// - whether a V-Tree Entry is currently attached
///
/// Nodes are colour-coded by state:
/// - green  — Terminal with a V-tree entry
/// - grey   — Terminal without a V-tree entry (orphaned leaf)
/// - yellow — Internal (both children present)
/// - orange — `SemiInternal` (one child present)///
/// # Panics
///
/// Panics if a terminal G-node is marked as having a V-tree entry but the
/// entry handle is `None` (internal invariant violation).
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
    queue.push_back(graph.gtree.root);

    while let Some(gid) = queue.pop_front() {
        let g = graph.gtree.nodes.get(gid.index());
        let idx = gid.index();
        let depth = gnode_depth_from_interval(g.lo(), g.hi(), N);
        let state = state_label(g.state());
        let has_entry = g.entry().is_some();

        let fillcolor = match g.state() {
            GState::Terminal if has_entry => "#c8e6c9", // green
            GState::Terminal => "#e0e0e0",              // grey
            GState::Internal => "#fff9c4",              // yellow
            GState::SemiInternal => "#ffe0b2",          // orange
        };

        let entry_note = if has_entry {
            format!("VEntry({})", g.entry().unwrap().index())
        } else {
            "no VEntry".to_string()
        };

        writeln!(
            out,
            "  G{idx} [label=\"G{idx}  [{:.0}, {:.0})\\nd={depth}  {state}  {entry_note}\\nown={:.0}  sum={:.0}\", \
             style=filled, fillcolor=\"{fillcolor}\"];",
            g.lo().to_f64(),
            g.hi().to_f64(),
            g.own().to_f64_approx(),
            g.sum().to_f64_approx(),
        )
        .unwrap();

        if let Some(left) = g.left() {
            queue.push_back(left);
        }
        if let Some(right) = g.right() {
            queue.push_back(right);
        }
    }

    writeln!(out).unwrap();

    // Edges (separate pass so all nodes are declared before edges).
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(graph.gtree.root);
    while let Some(gid) = queue.pop_front() {
        let g = graph.gtree.nodes.get(gid.index());
        let idx = gid.index();
        if let Some(left) = g.left() {
            writeln!(out, "  G{idx} -> G{} [label=\"L\"];", left.index()).unwrap();
            queue.push_back(left);
        }
        if let Some(right) = g.right() {
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

        match &vnode.kind() {
            VKind::Entry {
                gnode,
                is_exposed,
                is_evictable,
            } => {
                let g = graph.gtree.nodes.get(gnode.index());
                let g_depth = gnode_depth_from_interval(g.lo(), g.hi(), N);
                let g_state = state_label(g.state());
                writeln!(
                    out,
                    "  V{idx} [shape=box, style=filled, fillcolor=\"#bbdefb\", \
                     label=\"V{idx} Entry\\nG{}  [{:.0},{:.0}) d={g_depth} {g_state}\\n\
                     intensity={:.0}  exposed={is_exposed}  evict={is_evictable}\"];",
                    gnode.index(),
                    g.lo().to_f64(),
                    g.hi().to_f64(),
                    vnode.intensity().to_f64_approx(),
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
                    vnode.intensity().to_f64_approx(),
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

        if let VKind::Structural { children, .. } = &vnode.kind() {
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
