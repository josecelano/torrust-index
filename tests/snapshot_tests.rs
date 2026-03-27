//! Snapshot tests — regression guards for [`GvGraph`] internal state.
//!
//! Each scenario runs a deterministic sequence of operations and calls
//! [`insta::assert_snapshot!`] after every meaningful step.  The snapshots
//! capture both the full **G-tree** (every allocated node, showing the
//! geometric partition) and the **V-tree** (only the live/active nodes with
//! observations).  Storing both trees makes refactoring regressions obvious:
//! a diff in the G-tree means the partition structure changed; a diff in the
//! V-tree means the active-node set or its values changed.
//!
//! # Snapshot workflow
//!
//! **First run** (no `.snap` files yet — tests fail and create `.snap.new`):
//! ```bash
//! cargo test --test snapshot_tests
//! cargo insta review        # inspect diffs, press `a` to accept each
//! ```
//! Or auto-accept without interactive review:
//! ```bash
//! INSTA_UPDATE=new cargo test --test snapshot_tests
//! ```
//!
//! **After a refactoring** — snapshots catch unintended changes:
//! ```bash
//! cargo test --test snapshot_tests   # fails with diff if anything changed
//! cargo insta review                 # decide: accept or revert
//! ```
//!
//! Snapshot files live in `tests/snapshots/` and **must be committed** — they
//! are the source of truth.
//!
//! # Scenarios
//!
//! | Test | What it exercises |
//! |------|-------------------|
//! | [`scenario_simple_observe`] | Basic accumulation, no splits |
//! | [`scenario_split_triggered`] | Tree splitting when threshold is crossed |
//! | [`scenario_decay`] | Decay halves values without changing topology |
//! | [`scenario_two_hot_spots`] | Two independent clusters + decay |

use std::collections::HashMap;
use std::fmt::Write as _;

use torrust_mudlark::{Config, GNodeId, GState, GvGraph, StructuralConfig};

// ── Graph type ────────────────────────────────────────────────────────────────
// u32 coordinate, u64 value, N=16 → address space 0..65 535.
// Small N keeps node ranges short and readable in snapshot diffs.
type TestGraph = GvGraph<u32, u64, 16>;

#[must_use]
fn make_config() -> Config<u64> {
    Config {
        split_threshold: 5,
        structural: StructuralConfig {
            depth_create: 3,
            depth_evict: 8,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        },
    }
}

// ── G-tree DFS ────────────────────────────────────────────────────────────────

fn append_gnode(
    graph: &TestGraph,
    id: GNodeId,
    prefix: &str,
    connector: &str,
    child_prefix: &str,
    out: &mut String,
) {
    let Some(node) = graph.gnode_info(id) else {
        return;
    };
    let state = match node.state {
        GState::Terminal => 'T',
        GState::Internal => 'I',
        GState::SemiInternal => 'S',
    };
    writeln!(
        out,
        "{prefix}{connector}[d{d}] {s}..{e}  own={o}  sum={m}  {state}",
        d = node.depth,
        s = node.start,
        e = node.end,
        o = node.own,
        m = node.sum,
    )
    .expect("write to String is infallible");

    let Some(ch) = graph.gnode_children(id) else {
        return;
    };
    match (ch.left, ch.right) {
        (Some(l), Some(r)) => {
            append_gnode(
                graph,
                l,
                child_prefix,
                "├── ",
                &format!("{child_prefix}│   "),
                out,
            );
            append_gnode(
                graph,
                r,
                child_prefix,
                "└── ",
                &format!("{child_prefix}    "),
                out,
            );
        }
        (Some(only), None) | (None, Some(only)) => {
            append_gnode(
                graph,
                only,
                child_prefix,
                "└── ",
                &format!("{child_prefix}    "),
                out,
            );
        }
        (None, None) => {}
    }
}

// ── V-tree renderer (active nodes via layers()) ───────────────────────────────

struct ActiveNode {
    id: GNodeId,
    parent: Option<GNodeId>,
    start: u32,
    end: u32,
    own: u64,
    sum: u64,
    depth: u32,
    state: GState,
}

fn append_active_node(
    nodes: &[ActiveNode],
    children_of: &HashMap<usize, Vec<usize>>,
    idx: usize,
    prefix: &str,
    connector: &str,
    child_prefix: &str,
    out: &mut String,
) {
    let n = &nodes[idx];
    let state = match n.state {
        GState::Terminal => 'T',
        GState::Internal => 'I',
        GState::SemiInternal => 'S',
    };
    writeln!(
        out,
        "{prefix}{connector}[d{d}] {s}..{e}  own={o}  sum={m}  {state}",
        d = n.depth,
        s = n.start,
        e = n.end,
        o = n.own,
        m = n.sum,
    )
    .expect("write to String is infallible");

    let kids = children_of.get(&idx).cloned().unwrap_or_default();
    let n_kids = kids.len();
    for (i, ci) in kids.into_iter().enumerate() {
        let is_last = i == n_kids - 1;
        if is_last {
            append_active_node(
                nodes,
                children_of,
                ci,
                child_prefix,
                "└── ",
                &format!("{child_prefix}    "),
                out,
            );
        } else {
            append_active_node(
                nodes,
                children_of,
                ci,
                child_prefix,
                "├── ",
                &format!("{child_prefix}│   "),
                out,
            );
        }
    }
}

// ── Snapshot builder ──────────────────────────────────────────────────────────

/// Produce a deterministic, human-readable snapshot of the graph state.
///
/// Format:
/// ```text
/// total_sum: <value>
///
/// G-tree:
/// [d0] 0..65535  own=0  sum=<n>  I
/// ├── [d1] 0..32767  own=0  sum=0  T
/// └── [d1] 32768..65535  own=<n>  sum=<n>  S
///
/// V-tree (active nodes):
/// [d1] 32768..65535  own=<n>  sum=<n>  S
/// ```
#[must_use]
fn snapshot_of(graph: &TestGraph) -> String {
    let mut out = String::new();

    writeln!(out, "total_sum: {}", graph.total_sum()).expect("write to String is infallible");

    // Full G-tree — every allocated G-node, including empty/silent ones.
    writeln!(out).expect("write to String is infallible");
    writeln!(out, "G-tree:").expect("write to String is infallible");
    append_gnode(graph, graph.g_root(), "", "", "", &mut out);

    // V-tree — only the G-nodes that hold a live V-tree entry.
    // Hierarchy is reconstructed from G-tree parent links.
    writeln!(out).expect("write to String is infallible");
    writeln!(out, "V-tree (active nodes):").expect("write to String is infallible");

    let active: Vec<ActiveNode> = graph
        .layers()
        .map(|(_, node)| ActiveNode {
            id: node.gnode_id,
            parent: node.parent,
            start: node.start,
            end: node.end,
            own: node.own,
            sum: node.sum,
            depth: node.depth,
            state: node.state,
        })
        .collect();

    if active.is_empty() {
        writeln!(out, "  (empty)").expect("write to String is infallible");
        return out;
    }

    // Build id→index and parent→children maps.
    let mut id_to_idx: HashMap<GNodeId, usize> = HashMap::new();
    for (i, n) in active.iter().enumerate() {
        id_to_idx.insert(n.id, i);
    }
    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, n) in active.iter().enumerate() {
        match n.parent {
            Some(pid) => match id_to_idx.get(&pid) {
                Some(&pi) => children_of.entry(pi).or_default().push(i),
                None => roots.push(i),
            },
            None => roots.push(i),
        }
    }

    let n_roots = roots.len();
    for (i, ri) in roots.into_iter().enumerate() {
        let is_last = i == n_roots - 1;
        if n_roots == 1 {
            append_active_node(&active, &children_of, ri, "", "", "", &mut out);
        } else if is_last {
            append_active_node(&active, &children_of, ri, "", "└── ", "    ", &mut out);
        } else {
            append_active_node(&active, &children_of, ri, "", "├── ", "│   ", &mut out);
        }
    }

    out
}

// ── Scenarios ─────────────────────────────────────────────────────────────────

/// Three isolated observations at different coordinates.
///
/// Verifies that the G-tree accumulates values in the root and that the
/// V-tree correctly tracks which G-nodes have been observed.  With
/// `split_threshold = 5`, no splits occur here.
#[test]
fn scenario_simple_observe() {
    let mut graph = TestGraph::new(make_config());

    graph.observe(1_000_u32, 10_u64);
    insta::assert_snapshot!("simple_observe__01_first_coord", snapshot_of(&graph));

    graph.observe(2_000_u32, 5_u64);
    insta::assert_snapshot!("simple_observe__02_second_coord", snapshot_of(&graph));

    // Re-observe the first coordinate — accumulates, no split yet.
    graph.observe(1_000_u32, 3_u64);
    insta::assert_snapshot!(
        "simple_observe__03_revisit_first_coord",
        snapshot_of(&graph)
    );
}

/// Repeated observations at one coordinate until the split threshold is crossed.
///
/// With `split_threshold = 5` the 6th observation on the same G-node causes
/// the first split.  Snapshots capture the tree just before and just after the
/// split, and then after several more splits.
#[test]
fn scenario_split_triggered() {
    let mut graph = TestGraph::new(make_config());

    // 5 observations fill the threshold but don't yet split.
    for _ in 0..5 {
        graph.observe(1_000_u32, 1_u64);
    }
    insta::assert_snapshot!("split_triggered__01_at_threshold", snapshot_of(&graph));

    // 6th observation crosses the threshold → first split recorded.
    graph.observe(1_000_u32, 1_u64);
    insta::assert_snapshot!("split_triggered__02_after_first_split", snapshot_of(&graph));

    // Continue until deeper splits stabilise.
    for _ in 0..20 {
        graph.observe(1_000_u32, 1_u64);
    }
    insta::assert_snapshot!("split_triggered__03_deep_splits", snapshot_of(&graph));
}

/// Decay halves all values without altering the tree topology.
///
/// The G-tree structure (which nodes exist, their ranges, their depth) must be
/// identical before and after decay.  Only `own` and `sum` values change.
#[test]
fn scenario_decay() {
    let mut graph = TestGraph::new(make_config());

    for _ in 0..30 {
        graph.observe(1_000_u32, 1_u64);
    }
    insta::assert_snapshot!("decay__01_before_decay", snapshot_of(&graph));

    let root = graph.g_root();
    graph.decay(root, 0.5, 0.001);
    insta::assert_snapshot!("decay__02_after_half_decay", snapshot_of(&graph));

    graph.decay(root, 0.5, 0.001);
    insta::assert_snapshot!("decay__03_after_quarter_decay", snapshot_of(&graph));
}

/// Two independent hot spots grow separately then decay together.
///
/// Ensures that the tree correctly maintains two separate subtrees with
/// observations, and that decay proportionally reduces both.
#[test]
fn scenario_two_hot_spots() {
    let mut graph = TestGraph::new(make_config());

    for _ in 0..20 {
        graph.observe(1_000_u32, 1_u64);
    }
    insta::assert_snapshot!("two_hot_spots__01_first_cluster", snapshot_of(&graph));

    for _ in 0..15 {
        graph.observe(60_000_u32, 1_u64);
    }
    insta::assert_snapshot!(
        "two_hot_spots__02_second_cluster_added",
        snapshot_of(&graph)
    );

    let root = graph.g_root();
    graph.decay(root, 0.5, 0.001);
    insta::assert_snapshot!("two_hot_spots__03_after_decay", snapshot_of(&graph));
}
