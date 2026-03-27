# Refactoring Plan — Overview

Incremental plan to implement the improvements in
[../design-improvement-opportunities.md](../design-improvement-opportunities.md).
The structural model driving these decisions is in
[../design-model.md](../design-model.md).

---

## Guiding principles

- **One concern per commit.** Each step produces a passing, reviewable commit.
- **Tests are the safety net.** Run `cargo test --all-features` after every step.
  A failing test means the step is not done.
- **Compile-check early.** Use `cargo check --all-features` after mechanical
  renames before running the full suite.
- **Living document.** After each phase, fill in the Review checkpoint in the
  phase file. New findings may add steps or invalidate later phases.
- **No behaviour changes.** Every step is a pure refactoring. The integration
  tests, snapshot tests, and invariant checks are the oracle.
- **Free function vs method rule.** A function that takes `&mut GvGraph` (or
  `&mut GTree` / `&mut VTree`) has no reason to be free — it is a method spelled
  differently and belongs on that type. A function that takes `&GvGraph` belongs
  on the type only if the graph cannot work without it (internal query). External
  observers — diagnostics, visualisation, invariant checks — stay as free functions
  and must read the graph only through its public getters (not `pub(crate)` fields).

## How to run the test suite

```bash
cargo test --lib                          # unit tests only (fast)
cargo test --all-features                 # full suite
cargo test --no-default-features          # no-feature build
cargo test --no-default-features --features serde
cargo clippy --all-features -- -D warnings
```

---

## Progress

| Step                                                                              | Status | Title                                | Opportunity |
| --------------------------------------------------------------------------------- | ------ | ------------------------------------ | ----------- |
| [0.1](phase-0.md#step-01--record-baseline-test-counts)                            | ✅     | Record baseline test counts          | —           |
| [0.2](phase-0.md#step-02--confirm-clippy-is-clean)                                | ✅     | Confirm clippy is clean              | —           |
| [1.1](phase-1.md#step-11--packedchildrenv--childrenv-enum)                        | ✅     | `PackedChildren` → `Children` enum   | #6          |
| [1.2](phase-1.md#step-12--split-plateaus-into-a-separate-capability-trait)        | ✅     | Split `PlateauRead` trait            | #9          |
| [1.3](phase-1.md#step-13--tighten-visibility-of-internal-spatial-types)           | ✅     | Tighten visibility of internal types | #8          |
| [2.1](phase-2.md#step-21--extract-structuralconfig)                               | ✅     | Extract `StructuralConfig`           | #4          |
| [2.2](phase-2.md#step-22--re-export-and-clean-up)                                 | ✅     | Re-export and clean up               | #4          |
| [3.1](phase-3.md#step-31--encapsulate-gnodev-fields)                              | ✅     | Encapsulate `GNode` fields           | #10         |
| [3.2](phase-3.md#step-32--encapsulate-vnodev-fields)                              | ✅     | Encapsulate `VNode` fields           | #10         |
| [4.1](phase-4.md#step-41--define-gtree-as-a-struct-wrapper-no-logic-yet)          | ✅     | Introduce `GTree` struct             | #1, #2      |
| [4.2](phase-4.md#step-42--move-treegtreers-free-functions--gtree-methods)         | ✅     | Move gtree free fns → methods        | #1, #2      |
| [4.3](phase-4.md#step-43--move-splitev-tree-logic-into-gtree)                     | ✅     | Move G-tree mutation into `GTree`    | #1, #2      |
| [5.1](phase-5.md#step-51--define-vtreev-as-a-struct-wrapper)                      | ✅     | Introduce `VTree` struct             | #2, #3      |
| [5.2](phase-5.md#step-52--move-treevtreers-free-functions--vtree-methods)         | ✅     | Move vtree free fns → methods        | #2, #3      |
| [5.3](phase-5.md#step-53--move-violations-queue-ownership-into-vtree)             | ✅     | Move violations queue into `VTree`   | #3          |
| [5.4](phase-5.md#step-54--move-rebalance-entry-point-into-vtree)                  | ✅     | Move rebalance into `VTree`          | #3          |
| [6.1](phase-6.md#step-61--measure-the-cost-of-uncached-depth-computation)         | ✅     | Benchmark depth computation          | #7          |
| [6.2](phase-6.md#step-62--replace-cacheddepth-with-a-parallel-vecu32-in-vtree)    | ✅     | Move depth cache to `VTree`          | #7          |
| [6.3](phase-6.md#step-63--make-vnodev-copy-if-v-copy)                             | ✅     | Make `VNode<V>` `Copy`               | #7          |
| [7.1](phase-7.md#step-71--define-a-plateautracking-trait)                         | ⬜     | Define `PlateauTracking` trait       | #5          |
| [7.2](phase-7.md#step-72--implement-dynamicplateautracker-and-noopplateautracker) | ⬜     | Implement tracker types              | #5          |
| [7.3](phase-7.md#step-73--thread-the-tracker-through-gvgraph)                     | ⬜     | Thread tracker through `GvGraph`     | #5          |
| [7.4](phase-7.md#step-74--remove-cfgfeature--blocks-from-gvgraph-fields)          | ⬜     | Remove `#[cfg]` fields               | #5          |

**Status key:** ⬜ not started · 🔄 in progress · ✅ done · ⏸ blocked

---

## Dependency order

```
Phase 0  baseline (always first)

Phase 1  independent — low risk
Phase 2  independent — low risk
Phase 3  independent — easiest after Phase 1 stabilises node types

Phase 4  requires Phase 3
Phase 5  requires Phase 4

Phase 6  requires Phase 5
Phase 7  requires Phase 4 + 5
```

---

## Work log

_Reverse-chronological. Add an entry when a step is marked done._

| Date | Step | Commit | Notes |
| ---- | ---- | ------ | ----- |
| —    | —    | —      | —     |
