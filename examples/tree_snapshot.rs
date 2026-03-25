//! Step-by-step visual snapshots of the G-Tree and V-Tree internal state.
//!
//! Uses a tiny 4-bit address space (`N = 4`, coordinates `0..=15`) so the
//! trees stay small and easy to follow.  Three structural splits are triggered
//! as observations accumulate, producing a complete 3-level G-Tree and
//! demonstrating the V-Tree `contract` rebalancing step.
//!
//! After each observation the example writes three files into `docs/snapshots/`:
//!
//! | File | Contents |
//! |------|----------|
//! | `step_NN_<label>_gtree.dot` | Binary range-split tree (Graphviz) |
//! | `step_NN_<label>_vtree.dot` | Virtual intensity tree (Graphviz) |
//! | `step_NN_<label>_summary.txt` | Text table of all nodes |
//!
//! # Rendering (requires [Graphviz](https://graphviz.org/))
//!
//! Individual SVGs (one per step × tree type) are written to `docs/snapshots/svg/`:
//!
//! ```bash
//! ./docs/snapshots/render-svg.sh
//! ```
//!
//! A single scrollable **storyboard** — all steps side-by-side in one file — is
//! produced by the companion Python script (stdlib + Graphviz, no extra deps):
//!
//! ```bash
//! ./docs/snapshots/render-storyboard.py
//! # → docs/snapshots/svg/storyboard.svg
//! ```
//!
//! # Running
//!
//! ```bash
//! cargo run --example tree_snapshot
//! ```

use std::fs;
use std::path::Path;

use torrust_mudlark::{Config, GState, GvGraph};

// ---------------------------------------------------------------------------
// Graph type
// ---------------------------------------------------------------------------

/// A tiny map over a 4-bit address space (coordinates 0..=15).
///
/// `N = 4` means the domain is `[0, 2^4) = [0, 16)`.  The integer
/// coordinate type `u8` is more than wide enough.
type TinyMap = GvGraph<u8, u32, 4>;

// ---------------------------------------------------------------------------
// Snapshot helpers
// ---------------------------------------------------------------------------

/// Write the three snapshot files for the current tree state.
fn save_snapshot(step: usize, label: &str, graph: &TinyMap, dir: &Path) {
    let prefix = format!("step_{step:02}_{}", sanitize_label(label));

    let gtree_dot =
        torrust_mudlark::invariants::dump_gtree_dot(graph, &format!("Step {step}: {label}"));
    let vtree_dot =
        torrust_mudlark::invariants::dump_vtree_dot(graph, &format!("Step {step}: {label}"));
    let summary = build_summary(step, label, graph);

    fs::write(dir.join(format!("{prefix}_gtree.dot")), &gtree_dot).expect("write gtree dot");
    fs::write(dir.join(format!("{prefix}_vtree.dot")), &vtree_dot).expect("write vtree dot");
    fs::write(dir.join(format!("{prefix}_summary.txt")), &summary).expect("write summary");

    println!("  [{step:02}] {label:45}  → {prefix}_*");
}

/// Replace any character that would make an ugly filename with `_`.
fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        // Collapse consecutive underscores so filenames stay readable.
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Build a human-readable text table of all G-nodes in BFS / V-tree order.
fn build_summary(step: usize, label: &str, graph: &TinyMap) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "═══ Step {step}: {label} ═══\n\
         Total sum : {}\n\
         Node count: {}\n\n",
        graph.total_sum(),
        graph.node_count(),
    ));

    // Header.
    out.push_str(&format!(
        "{:>5}  {:<28}  {:>6}  {:>6}  {:<14}  {}\n",
        "v-dep", "range [lo … hi)", "own", "sum", "state", "gnode-id"
    ));
    out.push_str(&"─".repeat(80));
    out.push('\n');

    for (v_depth, node) in graph.layers() {
        let state = match node.state {
            GState::Terminal => "Terminal",
            GState::Internal => "Internal",
            GState::SemiInternal => "SemiInternal",
        };
        out.push_str(&format!(
            "{v_depth:>5}  [{:<12} …{:>12})  {:>6}  {:>6}  {:<14}  G({})\n",
            node.start,
            node.end,
            node.own,
            node.sum,
            state,
            node.gnode_id.index(),
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let dir = Path::new("docs/snapshots");
    fs::create_dir_all(dir).expect("failed to create docs/snapshots/");

    println!("Writing snapshots to {}/\n", dir.display());

    // ── Configuration ────────────────────────────────────────────────────────
    //
    // split_threshold = 2  → a node splits as soon as its accumulated sum
    //                        exceeds 2 (sum > 2, i.e. sum ≥ 3).
    // depth_create = 3     → the V-tree split gate allows Entry nodes at
    //                        v-depth ≤ 3 to trigger a G-tree split.
    //                        After two splits the deepest Entry nodes sit at
    //                        v-depth 3 (or 2 if kept as the isolate during a
    //                        contract), so depth_create = 3 guarantees all
    //                        second-generation children can still split.
    // depth_evict = 5      → eviction starts at G-tree depth 5 (must be
    //                        strictly greater than depth_create).
    // N = 4 (type-level)   → domain [0, 16).
    let mut graph = TinyMap::new(Config {
        split_threshold: 2_u32,
        depth_create: 3,
        depth_evict: 5,
        budget: None,
        alpha_relax: 0.5,
        bounded_eviction: false,
    });

    let mut step = 0;
    save_snapshot(step, "initial-empty", &graph, dir);

    // ── Observation sequence ─────────────────────────────────────────────────
    //
    // Legend for the "notes" below:
    //   G0 = the initial root G-node covering [0, 16)
    //   Split ① after step 3:  G0[0,16)  → G1[0,8)  + G2[8,16)   (bootstrap)
    //   Split ② after step 7:  G1[0,8)   → G3[0,4)  + G4[4,8)    (catalytic)
    //   Split ③ after step 10: G2[8,16)  → G5[8,12) + G6[12,16)  (catalytic)
    //     Before split ③ the V-tree Structural node that parents E1, E2, S2
    //     has three children; `contract` fires first, merging the two lightest
    //     children into a new Structural node before the split is applied.
    let observations: &[(u8, u32, &str)] = &[
        //  coord  value  label
        (2, 1, "obs coord=2  value=1"),
        (3, 1, "obs coord=3  value=1"),
        (2, 1, "obs coord=2  value=1 → SPLIT ① G0"), // G0 sum=3 > 2 → split
        (10, 1, "obs coord=10 value=1"),
        (2, 1, "obs coord=2  value=1"),
        (3, 1, "obs coord=3  value=1"),
        (2, 1, "obs coord=2  value=1 → SPLIT ② G1"), // G1[0,8) sum=3 > 2 → split
        (2, 1, "obs coord=2  value=1"),
        (10, 1, "obs coord=10 value=1"),
        (12, 1, "obs coord=12 value=1 → SPLIT ③ G2"), // G2[8,16) sum=3 > 2 → split
        (10, 1, "obs coord=10 value=1"),
        (12, 1, "obs coord=12 value=1"),
    ];

    for &(coord, value, label) in observations {
        step += 1;
        graph.observe(coord, value);
        save_snapshot(step, label, &graph, dir);
    }

    // ── Rendering instructions ───────────────────────────────────────────────
    println!(
        "\n{} snapshot sets written ({} files total).\n",
        step + 1,
        (step + 1) * 3
    );
    println!("Render all DOT files to SVG (requires Graphviz):");
    println!(
        "  cd {} && for f in *.dot; do dot -Tsvg \"$f\" -o \"${{f%.dot}}.svg\"; done",
        dir.display()
    );
    println!("\nOr render a single file, e.g.:");
    println!("  dot -Tsvg docs/snapshots/step_03_obs_coord_2_value_1___SPLIT____G0_gtree.dot \\");
    println!("       -o /tmp/step03_gtree.svg && xdg-open /tmp/step03_gtree.svg");
}
