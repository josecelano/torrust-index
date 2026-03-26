# Mudlark Isolated Review — Experiment Log

**Repo / branch:** [josecelano/torrust-index @ review/pr-832-mudlark-isolated](https://github.com/josecelano/torrust-index/tree/review/pr-832-mudlark-isolated)  
**Local path:** `/home/josecelano/Documents/git/committer/me/github/torrust/torrust-mudlark`  
**Reviewer:** @josecelano  
**Period:** March 2026

---

## Motivation

The main review was performed directly inside the `torrust-index` workspace (see
[REVIEW_PR_832.md](REVIEW_PR_832.md)). In parallel, a second track was run in an
isolated environment: only the Rust source files for the `mudlark` package were
extracted — no documentation, no comments, no tests — to ask a different question:
**is the code self-explanatory on its own?**

The isolated branch turned out to be a convenient sandbox for additional
experiments that would have added noise to the main review. All experiments
are tracked below.

---

## Experiment List

| #    | Name                              | Status  | Branch commit / doc                                    |
| ---- | --------------------------------- | ------- | ------------------------------------------------------ |
| E-1  | Code isolation & self-explanation | ✅ Done | Initial commit `c195fcf`                               |
| E-2  | Architecture diagrams             | ✅ Done | [`docs/architecture.md`][arch]                         |
| E-3  | Module-restructure proposal       | ✅ Done | [`docs/archive/refactor-module-structure.md`][refprop] |
| E-4  | Integration-test safety net       | ✅ Done | [`docs/archive/test-plan-pre-refactor.md`][testplan]   |
| E-5  | Test-coverage baseline            | ✅ Done | [`docs/coverage-baseline.md`][covbase]                 |
| E-6  | IP-range ban-detection use case   | ✅ Done | [`docs/use-cases/ip-range-ban-detection.md`][usecase]  |
| E-7  | TUI visualiser                    | ✅ Done | [`examples/tui_visualiser.rs`][tui]                    |
| E-8  | ANSI heat-map example             | ✅ Done | [`examples/ip_range_ban_detection.rs`][heatmap]        |
| E-9  | Snapshot tests                    | ✅ Done | [`docs/snapshot-tests.md`][snap]                       |
| E-10 | Module-restructure execution      | ✅ Done | commits `5aa6ca2`–`e5c870d`                            |
| E-11 | Unit-test coverage improvement    | ✅ Done | [`docs/unit-test-coverage-plan.md`][covplan]           |
| E-12 | Cyclomatic complexity analysis    | ✅ Done | [`docs/complexity-analysis.md`][ccx]                   |
| E-13 | Tree-snapshot + Graphviz DOT      | ✅ Done | [`examples/tree_snapshot.rs`][treesnap]                |
| E-14 | G-I5 bijection invariant          | ✅ Done | commit `c37d454`                                       |
| E-15 | Type-extraction refactor (P1–P8)  | ✅ Done | [`docs/module-reorganisation.md`][modreo]              |

[arch]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/architecture.md
[refprop]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/archive/refactor-module-structure.md
[testplan]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/archive/test-plan-pre-refactor.md
[covbase]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/coverage-baseline.md
[usecase]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/use-cases/ip-range-ban-detection.md
[tui]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/examples/tui_visualiser.rs
[heatmap]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/examples/ip_range_ban_detection.rs
[snap]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/snapshot-tests.md
[covplan]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/unit-test-coverage-plan.md
[ccx]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/complexity-analysis.md
[treesnap]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/examples/tree_snapshot.rs
[modreo]: https://github.com/josecelano/torrust-index/blob/review/pr-832-mudlark-isolated/docs/module-reorganisation.md

---

## Experiment Details

### E-1 — Code Isolation & Self-Explanation Test

**Question:** Can an AI agent understand `mudlark` purely from the Rust source —
without documentation, comments, or tests?

**Setup:** Copied every `.rs` file for the package into a fresh repo/branch,
stripped all doc-comments, inline comments, and test modules. Only executable
code remained — ~8 700 lines across 40 source files.

**Result:** Yes. The type system, trait bounds, and naming together provided
enough structural signal for the agent to reconstruct the purpose of each
module, explain the dual G-tree / V-tree design, and describe the contracts on
`observe`, `decay`, and `sample`. This was a useful calibration: the code
structure speaks for itself when comments are good, but the naming of some
internal functions (e.g. `plateau_after_evict`, `decompose_basis`) required
inference from their bodies.

---

### E-2 — Architecture Diagrams

Generated two PlantUML diagrams to make the component dependencies and execution
flow explicit:

- **Component architecture** — module dependency layers (foundation → nodes → spatial
  → tree → graph → diagnostics).
- **Call flow** — sequence of internal calls triggered by `observe()`, `decay()`,
  `sample()`, and `extract()`.

These aided the review of phases 3–4 in [REVIEW_PR_832.md](REVIEW_PR_832.md).

Diagrams: [`docs/architecture.md`][arch] (SVGs embedded inline).

---

### E-3 — Module-Restructure Proposal

The original package has a flat `src/` layout with ~20 files at the same depth.
Files use the module name as a namespace prefix (`graph_budget.rs`,
`graph_plateau.rs`, …) — exactly the pattern Rust subdirectories exist to solve.

Proposed a grouped layout:

```
src/
  arena.rs / handle.rs / traits/   ← foundation
  nodes/                           ← gnode, vnode
  spatial/                         ← view, plateau, pewei, contour_range
  tree/                            ← gtree, vtree
  graph/                           ← GvGraph + algorithm/
  diagnostics/                     ← invariants, diagnostic
```

Prerequisite: adequate test coverage before execution (every `use crate::` path
changes). Documented in [`docs/archive/refactor-module-structure.md`][refprop].

---

### E-4 — Integration-Test Safety Net

Before doing any restructuring, wrote a suite of integration tests in
`tests/integration.rs` covering the public API end-to-end. Key technique:
`check_all_invariants()` is called after every mutating operation, turning the
built-in invariant checker into a free structural oracle.

10 integration tests covering observe, split, decay, eviction, extraction, and
`range_sum`. See [`docs/archive/test-plan-pre-refactor.md`][testplan].

---

### E-5 — Test-Coverage Baseline

After the safety-net tests were in place, measured coverage before any further
changes:

| Metric    | Coverage |
| --------- | -------- |
| Lines     | 60.01%   |
| Functions | 57.43%   |
| Regions   | 61.49%   |

Tool: `cargo-llvm-cov 0.6.16`. Documented in [`docs/coverage-baseline.md`][covbase].

Plan created to reach ≥ 95% line coverage per module via unit tests written
directly in `#[cfg(test)]` blocks inside each source file.

---

### E-6 — IP-Range Ban-Detection Use Case

Documented a concrete application of `GvGraph` to the Torrust BitTorrent tracker
context: detecting coordinated attacks across a /24 (or /48) subnet.

A plain `HashMap<IpAddr, u32>` counter cannot surface distributed attacks where
no single IP crosses a threshold. `GvGraph` accumulates across the address space,
automatically splits hot sub-ranges at finer resolution via φ-bounded geometry,
and can answer "which /28 block is the hottest right now?" without enumerating
any IPs.

Added a runnable example `ip_range_ban_detection.rs` (with ANSI coloured heat-map
output) and the companion design-rationale document at
[`docs/use-cases/ip-range-ban-detection.md`][usecase].

---

### E-7 — TUI Visualiser

Added `examples/tui_visualiser.rs`: a live terminal UI that continuously feeds
random observations and renders both trees side-by-side using box-drawing
characters. Useful for watching how splitting, decay, and eviction interact in
real time.

Demonstrated in a screencast at `docs/use-cases/tui-visualiser-demo.mp4`.

---

### E-8 — ANSI Heat-Map Example

Part of the IP-range ban-detection example. After each "wave" of observations,
the example prints the /24 subnet as a 16 × 16 ANSI colour grid, colouring each
cell by its `range_sum`. Made the spatial adaptive behaviour of the structure
immediately visible during the review.

---

### E-9 — Snapshot Tests (insta)

Added `tests/snapshot_tests.rs` using the [`insta`](https://insta.rs) crate.
Each test runs a deterministic sequence of operations and calls
`insta::assert_snapshot!` after every meaningful step.

The snapshot format captures:

```
total_sum: N

G-tree:
[d<depth>] <start>..<end>  own=<own>  sum=<sum>  <state>

V-tree (active nodes):
[dN] ...
```

This made refactoring safe: any change to splitting, eviction, or decay logic
shows up as a diff in the committed `.snap` files. Documented in
[`docs/snapshot-tests.md`][snap].

---

### E-10 — Module-Restructure Execution

After having an adequate safety net (E-4, E-9), executed the refactor in five
sequential commits:

| Commit    | What moved                                                               |
| --------- | ------------------------------------------------------------------------ |
| `5aa6ca2` | `gnode.rs`, `vnode.rs` → `src/nodes/`                                    |
| `ea349b9` | `view.rs`, `plateau.rs`, `pewei.rs`, `contour_range.rs` → `src/spatial/` |
| `47783bc` | `gtree.rs`, `vtree.rs` → `src/tree/`                                     |
| `1c0cfa1` | `graph.rs` → `graph/mod.rs`; `graph_traits.rs` → `graph/traits.rs`       |
| `e5c870d` | `invariants.rs`, `diagnostic.rs` → `src/diagnostics/`                    |

All integration and snapshot tests passed after every step.

---

### E-11 — Unit-Test Coverage Improvement

Systematically wrote `#[cfg(test)] mod tests` blocks for each module, targeting
≥ 95% line coverage per file. Coverage improved from the post-refactor baseline
(60%) to:

| Metric    | Before | After  |
| --------- | ------ | ------ |
| Lines     | 60.01% | 86.96% |
| Functions | 57.43% | 96.67% |
| Regions   | 61.49% | 87.02% |

Total: 436 tests (422 unit + 10 integration + 4 snapshot). Plan and per-module
status at [`docs/unit-test-coverage-plan.md`][covplan].

The remaining gap (e.g. `invariants.rs` at 76.5%) is in branches that only fire
when the data structure is in an explicitly invalid state — deliberately hard to
exercise from outside.

---

### E-12 — Cyclomatic Complexity Analysis

Used [`rust-code-analysis-cli`](https://github.com/mozilla/rust-code-analysis)
(Mozilla, v0.0.25) to compute cyclomatic complexity, cognitive complexity, and
Halstead metrics per function across all of `src/`.

Top hotspots (CC = McCabe cyclomatic complexity):

| CC  | Cognitive | Function              | File                         |
| --- | --------- | --------------------- | ---------------------------- |
| 39  | **95**    | `plateau_after_evict` | `graph/algorithm/evict.rs`   |
| 32  | 37        | `evict_tip`           | `graph/algorithm/evict.rs`   |
| 26  | 46        | `normalize_plateaus`  | `graph/algorithm/plateau.rs` |
| 25  | 42        | `decay_selective`     | `graph/algorithm/decay.rs`   |

`plateau_after_evict` has cognitive complexity 95 — the highest bottleneck.
Full results at [`docs/complexity-analysis.md`][ccx]; raw JSON exported to
`metrics-output/`.

---

### E-13 — Tree-Snapshot + Graphviz DOT Export

Added `examples/tree_snapshot.rs`: runs a fixed sequence of observations (past
the split threshold) and exports the current `GvGraph` state as a Graphviz DOT
file. Supports visual diffing of tree structure before and after operations.

This was prompted by needing to inspect the exact shape of the tree when
investigating Finding #12 (post-evict `debug_assert`).

---

### E-14 — G-I5 Bijection Invariant

Added a new diagnostic invariant `G-I5`: every V-tree entry must correspond to
exactly one G-tree node, and vice versa (the two trees must be in bijection).
Extended the diagnostic example to trigger three successive splits and verify
the invariant at each step.

Commit `c37d454`.

---

### E-15 — Type-Extraction Refactor (P1–P8)

After the coverage work revealed that `graph/mod.rs` and `graph/algorithm/rebalance.rs`
were mixing large structs with algorithm code, a second-pass refactor extracted
standalone types into their own focused files:

| Step | Type extracted            | Destination                                |
| ---- | ------------------------- | ------------------------------------------ |
| P1   | `Config<V>`               | `src/graph/config.rs`                      |
| P2   | `GNodeChildren`           | `src/nodes/gnode.rs`                       |
| P3   | `ViolationSources`        | `src/graph/algorithm/violation_sources.rs` |
| P4–8 | Remaining parameter types | respective focused modules                 |

Goal: `mod.rs` files should be thin namespace routing layers, not implementation
layers. Documented in [`docs/module-reorganisation.md`][modreo].

---

## Key Findings from the Isolated Branch

These complement the findings in the main review (see [REVIEW_PR_832.md](REVIEW_PR_832.md)):

1. **The code is self-explanatory** — stripped of all comments and tests, an AI
   agent could correctly reconstruct the purpose and contracts of each module.
   This is a positive signal about the code quality.

2. **Module structure is improvable** — the flat layout with prefix-based naming
   is a well-known Rust anti-pattern and was straightforwardly refactorable.
   Proposed as a follow-up to Cameron in Finding #8 under Open Items.

3. **Coverage ceiling at ~87%** — the remaining 13% is dominated by
   `invariants.rs` and `plateau.rs` branches that are only reachable when the
   data structure is in an invalid state. This confirms Finding #12 (post-evict
   `debug_assert` in `plateau.rs:127`).

4. **`plateau_after_evict` is the complexity hotspot** — CC=39, cognitive=95.
   This corroborates Finding #9 (f64 drift) and Finding #12.
