# Complexity Reduction Plan — Round 2

> Archived predecessor: [`docs/archive/complexity-reduction-plan.md`](archive/complexity-reduction-plan.md)
>
> Round 1 completed all 16 items (P1–P16, commit range `731efed`–`988e48d`).
> This document records the remaining complexity found during a fresh codebase sweep.

---

## How to read this plan

Items are numbered **Q1–Q12** (Q for _quality_, to distinguish from the P-series).
Each item has:

- **File / line estimate** — primary location
- **Problem** — concrete description of what makes the code hard to read or change
- **Proposal** — small, testable change with clear success criteria
- **Effort** — LoC removed or reorganised, risk of correctness change
- **Impact** — readability / discoverability gain

---

## Items

### Q1 — `gv_graph.rs`: Extract capacity-parameter calculation from `new()`

**File**: `src/graph/gv_graph.rs` (~line 85)

**Problem**: `GvGraph::new()` is ~120 lines. The middle section — computing
`headroom`, `soft_limit`, and the `convergence_bound` check — is a non-trivial
mathematical derivation mixed directly into struct-field assignment. It is not
reusable and not testable in isolation.

```rust
let depth_buffer = config.depth_evict - config.depth_create;
let headroom = 3usize.pow(depth_buffer + 1);
let convergence_bound = 2 * (live_depth_create as usize).saturating_sub(1);
let required_headroom = headroom.max(convergence_bound);
let soft_limit = config.budget.map(|b| { ... });
```

**Proposal**: Extract a private `fn compute_capacity(config: &Config<V>) -> Capacity`
(or equivalent named struct) that returns `(depth_buffer, headroom, soft_limit)`.
Add a `///` doc comment explaining why `3^(depth_buffer+1)` and the
convergence bound are taken as headroom. This makes the capacity logic:

- standalone-testable
- clearly separated from arena construction

---

### Q2 — `rebalance.rs`: Extract safety-net error dump from `rebalance()`

**File**: `src/graph/algorithm/rebalance.rs` (~line 420)

**Problem**: The `rebalance()` loop body contains a 30-line safety-net clause
that dumps the violation queue tail and either panics (`debug_assertions`) or
logs and breaks (`release`). This handling is purely error-path code, unrelated
to the loop's happy path, and makes the main loop body much harder to scan.

**Proposal**: Extract to:

```rust
fn handle_iteration_limit_exceeded(
    vnodes: &Arena<VNode<V>>,
    violations: &[VNodeId],
    iterations: u32,
    max_iterations: u32,
    resolved: u32,
    current: VNodeId,
) -> ! | ()
```

The main loop then reads clearly as: _pop → skip-dead → skip-resolved →
call resolve → optionally call handler_. Zero algorithmic change.

---

### Q3 — `observe.rs`: Add phase comments to `observe()`

**File**: `src/graph/algorithm/observe.rs` (~line 7)

**Problem**: `observe()` sequences seven distinct phases:

1. Route and accumulate into the G-node
2. Propagate intensity up the V-tree; enqueue violations
3. Recompute G-tree sums
4. Update the plateau mirror
5. Attempt a G-node split; rebalance
6. Adjust depth gates
7. Eviction (if over soft limit)
8. Normalise plateaus; repair P-I4

None of these phases are annotated. Reading the function requires knowing the
algorithm to understand the ordering constraints (e.g. why split happens
_before_ rebalance, and rebalance _before_ depth-gate adjustment).

**Proposal**: Add `// ── Phase N: <name> ──` banner comments (matching the
style used in `evict_tip()`) for each phase. No code changes. Estimated: 8
one-line comments.

---

### Q4 — `rebalance.rs`: Move `Nd`/`Ch`/`Ctx` display helpers to a sub-module

**File**: `src/graph/algorithm/rebalance.rs` lines 14–72

**Problem**: `rebalance.rs` opens with 70 lines of `Display` formatter
structs (`Nd`, `Ch`, `Ctx`) before reaching any algorithm code. `rebalance.rs`
should lead with `is_violated` and `contract`; formatting infrastructure is
orthogonal.

`Nd` is `pub`, used by `split.rs` and `diagnostics/diagnostic.rs`. `Ch` is
`pub(super)`, used by `promote.rs`. `Ctx` is `pub` but only used internally.

**Proposal**: Create `src/graph/algorithm/fmt.rs` containing the three
structs. Re-export `Nd` and `Ctx` from `rebalance.rs` via
`pub use super::fmt::{Nd, Ctx};` and keep `Ch` as `pub(super)` with a re-export
in `rebalance.rs`. Add `pub(crate) mod fmt;` to `algorithm/mod.rs`.

This moves ~70 lines out of `rebalance.rs` and groups formatting concerns
together.

---

### Q5 — `gv_graph.rs`: Remove or conditionalise dead `uniform_contour_depth_of`

**File**: `src/graph/gv_graph.rs` line 18

**Problem**: `uniform_contour_depth_of` is marked `#[allow(dead_code)]` and
has no callers in the main library or tests. Dead code in a main module is
maintenance debt — it must be understood during every review even though it
has no effect.

**Proposal**: Either:

- Delete the function (preferred if it has no planned use), or
- Move it to `diagnostics/` under `#[cfg(test)]` or `#[allow(dead_code)]`
  with a comment explaining its intended diagnostic purpose.

If it belongs to a future feature, add a `// TODO(feature X)` comment.

---

### Q6 — `split.rs`: Break `catalytic_split()` into annotated phases

**File**: `src/graph/algorithm/split.rs` (~line 154)

**Problem**: `catalytic_split()` carries `#[allow(clippy::too_many_lines)]`
and is ~100 lines. The function:

1. Allocates G-children and V-entries
2. Computes depth metadata (with repeated `DEPTH_STALE` guard idioms)
3. Builds the new structural V-node `s`
4. Wires sibling intensities
5. Slots `s` into the parent's child list
6. Propagates flags up the V-tree
7. Updates plateau state and G-tree counts

These phases are not labelled. The depth-stale guard (`if depth == DEPTH_STALE
{ DEPTH_STALE } else { depth + 1 }`) appears three times inline; it belongs
in a one-line helper `fn depth_plus_one(d: u32) -> u32`.

**Proposal**:

1. Add `// ── Phase N: <name> ──` banner comments.
2. Extract `fn depth_plus_one(d: u32) -> u32` (trivial).
3. No structural reshaping — `catalytic_split` stays as one function.

---

### Q7 — `decay.rs`: Document intensity-update order asymmetry

**File**: `src/graph/algorithm/decay.rs`

**Problem**: `decay_uniform()` updates V-intensities **during** the G-tree
walk (inside the loop), then calls `recompute_all_v_intensities`. `decay_selective()`
updates G-`own` values in a first pass, recomputes G-sums, then updates
V-intensities in a **second** pass. This asymmetry is a subtle correctness
constraint (uniform needs immediate V values for the V-sum recompute call),
not apparent from the code.

Separately, `depth_attenuation_factors(att, q, depth_range)` evaluates
$f(d) = \exp\!\bigl[\ln(\text{att}) \cdot (q \cdot t + 1)\bigr]$
where $t = 2d/D - 1 \in [-1, 1]$. This mathematical contract is invisible.

**Proposal**:

1. Add a `///` doc comment to `depth_attenuation_factors` with the LaTeX
   formula and a short description of what `q` controls (q=0 → uniform
   across depth; q=1 → maximum taper from root to leaf).
2. Add a `// NOTE:` comment in both `decay_uniform` and `decay_selective`
   explaining the intensity-update ordering constraint.
3. No code changes — documentation only.

---

### Q8 — `pewei.rs`: Document `descend()` and `RegionLookup`

**File**: `src/spatial/pewei.rs` (~line 168)

**Problem**: `descend()` is an ~80-line recursive function with a 4-way match
on `(left_ref, right_ref)`. Each arm handles a different partial-coverage
case where one or both sub-regions are not represented in the lookup. The
energy remainder calculation in the asymmetric arms
(`V::sub(V::sub(total, baseline), child_total)`) is non-trivial without
understanding what `total` vs `baseline` represent. There is no module-level
doc explaining the Pewei data model.

`RegionLookup` is an internal struct with no doc. Its `build()` converts
layers into depth-indexed sorted buckets — the purpose (O(log n) start-address
lookup per depth) is useful to know.

**Proposal**:

1. Add a module-level `//!` doc block explaining:
   - What a `Pewei` is (layered snapshot of the G-tree at different decay
     epochs)
   - The role of `Terminal` and `Transition` nodes
   - How `reconstruct(max_layer)` uses `descend()` to re-materialise spans
2. Add a `/// ...` comment to `RegionLookup::build()` explaining the depth-
   bucket structure.
3. Add inline comments in `descend()` labelling what each match arm means
   ("both children present", "only left child present", etc.) and what
   the `remainder` calculation computes.
4. No code changes — documentation only.

---

### Q9 — `diagnostics/invariants.rs`: Group invariant checks in `check_all_invariants`

**File**: `src/diagnostics/invariants.rs` (~line 54)

**Problem**: `check_all_invariants()` lists 18 flat function calls with no
grouping. The function is 40 lines of opaque procedure names. There is no way
to tell which invariants relate to the G-tree, V-tree, node accounting, or
plateaus without reading every function.

**Proposal**: Group the 18 calls into four private helper functions:

```rust
fn check_g_tree_invariants(graph, errors);  // G-I1, G-I2, G-I4, G-I5
fn check_v_tree_invariants(graph, errors);  // V-I1..V-I7
fn check_accounting_invariants(graph, errors); // clean_accounting, counts, budget
fn check_plateau_invariants(graph, errors);   // P-I1..P-I5 (feature-gated)
```

`check_all_invariants()` then has 4 calls instead of 18. Each helper
documents its group with a `///` comment. `check_plateau_only()` reuses
`check_plateau_invariants()`.

---

### Q10 — `vtree.rs`: Add module-level doc

**File**: `src/tree/vtree.rs`

**Problem**: `vtree.rs` is 550 lines of pure algorithmic code with no
module-level documentation. A reader must scan function names to understand
what the module contains. After Round 1, the function names are clear, but
the relationship between them (e.g. when to call `propagate_v_sums` vs
`recompute_all_v_intensities`) is only documented via the `///` comments added
to individual functions.

**Proposal**: Add a `//!` module-level block covering:

- What a V-tree is and its role relative to the G-tree
- The two structural node kinds (`Entry`, `Structural`)
- The three main update patterns: point update + propagate, full post-order
  recompute, depth invalidation + lazy recompute
- A cross-reference to `rebalance.rs` for how violations are defined

No code changes — documentation only.

---

### Q11 — `rebalance.rs`: Annotate phases in `resolve()`

**File**: `src/graph/algorithm/rebalance.rs` (~line 282)

**Problem**: `resolve()` is ~100 lines dispatching between two major paths
(standard-promote when `c` is a 2-child structural node; skip/legacy-promote
otherwise). Each path has sub-phases (optional g-contraction, legacy vs skip
choice). The `tracing::debug!` calls name some transitions but not all, and
the overall structure is not documented at the top of the function.

**Proposal**: Add:

1. A `///` doc comment on `resolve()` describing the two paths and the
   conditions that select each one.
2. `// ── Path A: standard promote ──` and `// ── Path B: skip/legacy promote ──`
   banner comments inside the if/else.
3. No code changes.

---

### Q12 — `gv_graph.rs`: Split `impl` block into logical sections

**File**: `src/graph/gv_graph.rs` (~line 82)

**Problem**: The single large `impl GvGraph` block mixes:

- Construction (`new()`)
- Const-time accessors (`node_count()`, `depth_create()`, `headroom()`, etc.)
- Non-trivial queries (`gnode_info()`, `gnode_children()`, `is_ancestor_of()`)

There are ~28 accessor methods (most 3–5 lines) packed between `new()` and
the diagnostic helpers. There are no separator comments to guide navigation.

**Proposal**: Add `// ── Accessors ──`, `// ── Construction ──`, and
`// ── Queries ──` banner comments to divide the impl block visually. No
restructuring of the code itself. Alternatively, split into three `impl`
blocks if that's the local convention (check existing files first).

---

## Priority table

| ID  | Title                                                 | Impact | Risk |
| --- | ----------------------------------------------------- | ------ | ---- |
| Q3  | Phase comments in `observe()`                         | High   | None |
| Q11 | Phase comments / doc in `resolve()`                   | High   | None |
| Q8  | Document `descend()` and `RegionLookup`               | High   | None |
| Q7  | Document decay intensity-update ordering              | High   | None |
| Q10 | Module-level doc for `vtree.rs`                       | Medium | None |
| Q1  | Extract capacity params from `new()`                  | Medium | Low  |
| Q2  | Extract safety-net dump from `rebalance()`            | Medium | Low  |
| Q6  | Annotate phases + `depth_plus_one` in `split.rs`      | Medium | Low  |
| Q9  | Group invariant calls in `check_all_invariants`       | Medium | Low  |
| Q4  | Move `Nd`/`Ch`/`Ctx` to `fmt.rs` sub-module           | Low    | Low  |
| Q5  | Remove/conditionalise dead `uniform_contour_depth_of` | Low    | None |
| Q12 | Section banners in `gv_graph.rs` impl block           | Low    | None |

---

## Tracking

| ID  | Status      | Notes                                                                       |
| --- | ----------- | --------------------------------------------------------------------------- |
| Q1  | Not started |                                                                             |
| Q2  | Not started |                                                                             |
| Q3  | Done        | `89ab927` — phase banners in `observe()`                                    |
| Q4  | Not started |                                                                             |
| Q5  | Not started |                                                                             |
| Q6  | Not started |                                                                             |
| Q7  | Done        | `922f2f3` — formula doc + NOTE comments in decay                            |
| Q8  | Done        | `61ad514` — module doc, `RegionLookup::build` doc, `descend()` arm comments |
| Q9  | Not started |                                                                             |
| Q10 | Done        | `9ff3ee6` — module-level doc for `vtree.rs`                                 |
| Q11 | Done        | `9ddd9d3` — doc comment + path banners in `resolve()`                       |
| Q12 | Not started |                                                                             |
