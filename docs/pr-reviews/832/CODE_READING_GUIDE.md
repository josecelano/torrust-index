# Mudlark Code Reading Guide

A structured path through the ~25,500 lines of `packages/mudlark/src/`.
Read each section in order. After each file, ask questions before moving on —
understanding the earlier layers is required to make sense of the later ones.

---

## Stage 1 — Design documents

Read these before touching any `.rs` file. They map the _why_ to the _what_.

| #   | File                                    | What you learn                                                                                                              |
| --- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 1.1 | `packages/mudlark/docs/idea.md`         | The core problem: why a standard segment tree is not enough, what φ-bounded eviction means, the dual-tree design motivation |
| 1.2 | `packages/mudlark/docs/api.md`          | The public contract: `observe`, `range_sum`, `sample`, `decay`, `total_sum` — what callers can rely on                      |
| 1.3 | `packages/mudlark/docs/architecture.md` | How the modules are split and why                                                                                           |
| 1.4 | `packages/mudlark/adr/024-*.md`         | ADR-M-024: why the G-Tree and V-Tree are separate                                                                           |
| 1.5 | `packages/mudlark/adr/031-*.md`         | ADR-M-031: the plateau dirty-flag design                                                                                    |
| 1.6 | `packages/mudlark/adr/032-*.md`         | ADR-M-032: the three-surface public API model                                                                               |
| 1.7 | `packages/mudlark/adr/036-*.md`         | ADR-M-036: `GNodeInfo` and why it was added                                                                                 |

**Stop here and make sure you can answer:**

- What is a "G-node" and what is a "V-node"? Why are there two trees?
- What does `observe(coord, delta)` promise to the caller?
- What does `range_sum(a..b)` return — and what is it _not_ guaranteed to return exactly?
- What is φ-bounded eviction? Why does it matter for memory safety?

---

## Stage 2 — Worked example

The single best entry point into the code. Walk through a real lifecycle.

| #   | File                          | What you learn                                                                                        |
| --- | ----------------------------- | ----------------------------------------------------------------------------------------------------- |
| 2.1 | `src/tests/worked_example.rs` | A step-by-step annotated run: insert, split, query, decay. Assertions explain _why_ each state holds. |

**Stop here and make sure you can answer:**

- What happens to `total_sum` after a G-node split?
- At what point does a V-node get created?
- Which observation triggers the first split in the example?

---

## Stage 3 — Traits (the type language)

Every other file speaks in terms of these traits. Read them first so the
type signatures in later files are not opaque.

| #    | File                                                           | What you learn                                                  |
| ---- | -------------------------------------------------------------- | --------------------------------------------------------------- |
| 3.1  | `src/traits/mod.rs`                                            | Re-exports and overview                                         |
| 3.2  | `src/traits/coordinate.rs`                                     | What a coordinate is: dyadic ranges, midpoints, bit-depth       |
| 3.3  | `src/traits/accumulator.rs`                                    | `Accumulator<V>`: zero, add, scale_by — the value type contract |
| 3.4  | `src/traits/observation.rs`                                    | How an `O` (observation type) is accumulated into `V`           |
| 3.5  | `src/traits/proratable.rs`                                     | Pro-rating a value across a sub-range                           |
| 3.6  | `src/traits/temporal_decay.rs`                                 | How a value decays over time                                    |
| 3.7  | `src/traits/spatial_read.rs`                                   | Read surface: `range_sum`, `total_sum`, `sample`                |
| 3.8  | `src/traits/spatial_write.rs`                                  | Write surface: `observe`, `decay`                               |
| 3.9  | `src/traits/rng.rs`                                            | The deterministic RNG trait used throughout tests               |
| 3.10 | `src/traits/weighable.rs`, `attenuatable.rs`, `inspectable.rs` | Minor supporting traits                                         |

**Stop here and make sure you can answer:**

- What is the difference between `C` (coordinate type) and `V` (value type) in `GvGraph<C, V, N>`?
- What does `scale_by` do and why does it matter for `range_sum`?
- Why is pro-rating needed at all?

---

## Stage 4 — Data model (nodes and arena)

The concrete types that live in memory.

| #   | File            | What you learn                                                                                    |
| --- | --------------- | ------------------------------------------------------------------------------------------------- |
| 4.1 | `src/arena.rs`  | Bump allocator: how both trees allocate nodes; why indices are stable                             |
| 4.2 | `src/gnode.rs`  | `GNode`: fields (`sum`, `own`, `parent`, `left`, `right`), node states (Root, Internal, Terminal) |
| 4.3 | `src/vnode.rs`  | `VNode`: mirror entry in the V-Tree; gap representation                                           |
| 4.4 | `src/handle.rs` | `GNodeId` / `VNodeId` typed index wrappers; why typed indices instead of `usize`                  |

**Stop here and make sure you can answer:**

- What does `gnode.own` represent vs `gnode.sum`?
- What does it mean for a G-node to be a "terminal"?
- Why does a G-node have a `parent` pointer but not the V-node?
- What is an "arena" in this context and why use one instead of `Box<Node>`?

---

## Stage 5 — The `GvGraph` struct

The top-level type and the observe entry point.

| #   | File                      | What you learn                                                           |
| --- | ------------------------- | ------------------------------------------------------------------------ |
| 5.1 | `src/graph.rs`            | `GvGraph` struct fields, `new()`, `Config`, and the `observe()` dispatch |
| 5.2 | `src/observe.rs`          | The observe mutation path: root walk, delta propagation, split trigger   |
| 5.3 | `src/tests/graph.rs`      | Unit tests for graph construction and basic observe behaviour            |
| 5.4 | `src/tests/graph_init.rs` | Edge cases around initialisation                                         |
| 5.5 | `src/tests/observe.rs`    | Observe-specific tests                                                   |

**Stop here and make sure you can answer:**

- What fields does `GvGraph` hold at the top level?
- What is `Config.split_threshold` and what does it control?
- What is the sequence of steps inside a single call to `observe(coord, delta)`?
- When is the split path entered?

---

## Stage 6 — The four core operations

The algorithmic heart. Read in this order — each builds on the previous.

### 6a — Split

| #   | File                 | What you learn                                                                                                   |
| --- | -------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 6.1 | `src/split.rs`       | A terminal G-node exceeds `split_threshold`: it is bisected, its `own` value is pro-rated to left/right children |
| 6.2 | `src/tests/split.rs` | Split invariant tests                                                                                            |

**Key question:** after a split, how is the parent's `sum` maintained?

### 6b — Rebalance

| #   | File                            | What you learn                                                                                                        |
| --- | ------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| 6.3 | `src/rebalance.rs`              | The hardest file (1478 lines): how the G-Tree restructures after splits to maintain the φ-bound; V-Tree pointer fixup |
| 6.4 | `src/tests/rebalance.rs`        | Rebalance correctness tests                                                                                           |
| 6.5 | `src/tests/rebalance_stress.rs` | Stress tests                                                                                                          |

**Key question:** what is the φ-bound and how does rebalance enforce it?

### 6c — Eviction

| #   | File                 | What you learn                                                                         |
| --- | -------------------- | -------------------------------------------------------------------------------------- |
| 6.6 | `src/evict.rs`       | When the budget is exceeded: which nodes are evicted, how their value is merged upward |
| 6.7 | `src/tests/evict.rs` | Eviction invariant tests                                                               |

**Key question:** what invariant guarantees that `total_sum` does not change after eviction?

### 6d — Query

| #   | File                      | What you learn                                            |
| --- | ------------------------- | --------------------------------------------------------- |
| 6.8 | `src/graph_query.rs`      | `range_sum`, `total_sum`, `sample`, `get` — the read path |
| 6.9 | `src/tests/structural.rs` | Structural invariant tests that span query + mutation     |

**Key question:** why is `range_sum(a..b)` an _approximation_ rather than an exact answer? When is it exact?

---

## Stage 7 — PEWEI pro-rating

The algorithm that distributes a node's value across sub-ranges.

| #   | File                                   | What you learn                                                                                 |
| --- | -------------------------------------- | ---------------------------------------------------------------------------------------------- |
| 7.1 | `packages/mudlark/docs/idea.md` §PEWEI | Re-read the PEWEI section now that you understand the tree shape                               |
| 7.2 | `src/pewei.rs`                         | The reconstruction algorithm: walking the tree to estimate the density at arbitrary sub-ranges |
| 7.3 | `src/tests/pewei.rs`                   | PEWEI correctness tests including the f64 case                                                 |

**Key question:** what inputs does `pewei::reconstruct` take and what does it output?

---

## Stage 8 — Decay

| #   | File                           | What you learn                                                 |
| --- | ------------------------------ | -------------------------------------------------------------- |
| 8.1 | `src/decay.rs`                 | How `decay(factor, horizon)` attenuates all values in the tree |
| 8.2 | `src/tests/decay.rs`           | Decay invariant tests                                          |
| 8.3 | `src/tests/decay_f64_depth.rs` | f64-specific depth / saturation edge cases                     |

---

## Stage 9 — V-Tree and G-Tree structure

| #   | File                        | What you learn                                                                   |
| --- | --------------------------- | -------------------------------------------------------------------------------- |
| 9.1 | `src/gtree.rs`              | G-Tree structural helpers: depth queries, ancestor checks, range-to-node mapping |
| 9.2 | `src/vtree.rs`              | V-Tree: the gap structure, how V-nodes mirror G-node leaves                      |
| 9.3 | `src/tests/gtree.rs`        | G-Tree unit tests                                                                |
| 9.4 | `src/tests/vtree.rs`        | V-Tree unit tests                                                                |
| 9.5 | `src/tests/vnode.rs`        | V-node edge cases                                                                |
| 9.6 | `src/tests/spiked_vtree.rs` | V-Tree under pathological input                                                  |

---

## Stage 10 — Plateau / contour tracking

The most complex part. Only attempt after all previous stages.

| #    | File                                     | What you learn                                                                              |
| ---- | ---------------------------------------- | ------------------------------------------------------------------------------------------- |
| 10.1 | `packages/mudlark/docs/idea.md` §plateau | The contour / plateau concept                                                               |
| 10.2 | `src/plateau.rs`                         | `PlateauBasis`: the bookkeeping structure for which nodes form a plateau                    |
| 10.3 | `src/contour_range.rs`                   | `compute_plateau_energy`: how the energy / intensity of a contour is calculated             |
| 10.4 | `src/graph_plateau.rs`                   | The full plateau lifecycle: create, update on observe, fixup after split/rebalance/eviction |
| 10.5 | `src/tests/plateau.rs`                   | Plateau unit tests                                                                          |
| 10.6 | `src/tests/semi_internal_plateau.rs`     | The semi-internal plateau edge case (1325 lines — most thorough test file)                  |

**Note:** this is where Finding #9 from the review lives — `graph_plateau.rs:1384` uses `assert_eq!` on f64 sums that diverge by ~1 ULP after a split.

---

## Stage 11 — Invariants (second specification)

| #    | File                | What you learn                                                                                                                                                        |
| ---- | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 11.1 | `src/invariants.rs` | `assert_invariants`: every structural property the author considers essential. Read as a second specification — each assertion is a theorem about the data structure. |

---

## Stage 12 — Supporting infrastructure

By this point the hard code is behind you. These files are easier.

| #    | File                   | What you learn                                                                       |
| ---- | ---------------------- | ------------------------------------------------------------------------------------ |
| 12.1 | `src/view.rs`          | The `View` / `Node` read-only projection used by the diagnostic and extract surfaces |
| 12.2 | `src/diagnostic.rs`    | Debug/diagnostic output helpers                                                      |
| 12.3 | `src/graph_budget.rs`  | Budget enforcement: node count tracking                                              |
| 12.4 | `src/graph_extract.rs` | The extract surface: iterating the tree as a sequence of (range, value) pairs        |
| 12.5 | `src/graph_traits.rs`  | Trait impls for `GvGraph` (the three surfaces wired together)                        |
| 12.6 | `src/testing/`         | Test helpers: `Plan`, `Builder`, configs, `TestLcgRng`, preset scenarios             |

---

## Reference documents

These are useful to keep open as you read:

| Document                                               | Use                                                  |
| ------------------------------------------------------ | ---------------------------------------------------- |
| [GLOSSARY.md](GLOSSARY.md)                             | Term definitions — check here when a name is unclear |
| [MUDLARK_EXPLAINED.md](MUDLARK_EXPLAINED.md)           | Non-specialist overview — re-read after each stage   |
| [DESIGN_ALTERNATIVES.md](DESIGN_ALTERNATIVES.md)       | Why not a Fenwick tree / segment tree / etc.         |
| [COORDINATE_ENGINEERING.md](COORDINATE_ENGINEERING.md) | How non-integer domains are encoded as coordinates   |
| `packages/mudlark/docs/performance.md`                 | Bench results and complexity claims to validate      |

---

## Progress tracker

| Stage | Topic                     | Status |
| ----- | ------------------------- | ------ |
| 1     | Design documents          | ⬜     |
| 2     | Worked example            | ⬜     |
| 3     | Traits                    | ⬜     |
| 4     | Data model                | ⬜     |
| 5     | `GvGraph` + observe       | ⬜     |
| 6a    | Split                     | ⬜     |
| 6b    | Rebalance                 | ⬜     |
| 6c    | Eviction                  | ⬜     |
| 6d    | Query                     | ⬜     |
| 7     | PEWEI                     | ⬜     |
| 8     | Decay                     | ⬜     |
| 9     | V-Tree / G-Tree structure | ⬜     |
| 10    | Plateau / contour         | ⬜     |
| 11    | Invariants                | ⬜     |
| 12    | Supporting infrastructure | ⬜     |
