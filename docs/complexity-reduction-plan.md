# Complexity Reduction Plan

## Objective

Systematically improve the most complex files in `src/` along four axes:

1. **Reduce cyclomatic / cognitive complexity** — split long functions into focused helpers.
2. **Improve readability and maintainability** — make intent visible through names and structure.
3. **Improve testability** — extract pure functions that can be unit-tested without a full graph.
4. **Fix structural weaknesses** — missing abstractions, static-vs-runtime safety, dead code.

---

## Reference: Top Files by Complexity

| CC  | Cog | SLOC | File                               |
| --- | --- | ---- | ---------------------------------- |
| 241 | 215 | 1452 | `src/graph/algorithm/rebalance.rs` |
| 238 | 287 | 1554 | `src/graph/algorithm/plateau.rs`   |
| 101 | 73  | 673  | `src/graph/algorithm/query.rs`     |
| 91  | 143 | 600  | `src/graph/algorithm/evict.rs`     |
| 89  | 82  | 591  | `src/diagnostics/diagnostic.rs`    |
| 70  | 51  | 527  | `src/tree/vtree.rs`                |
| 67  | 0   | 518  | `src/traits/coordinate.rs`         |
| 63  | 64  | 416  | `src/graph/algorithm/decay.rs`     |

Worst individual functions (CC > 20):

| CC  | Cog | Function                    | File            |
| --- | --- | --------------------------- | --------------- |
| 39  | 95  | `plateau_after_evict`       | `evict.rs`      |
| 32  | 37  | `evict_tip`                 | `evict.rs`      |
| 26  | 46  | `normalize_plateaus`        | `plateau.rs`    |
| 25  | 42  | `decay_selective`           | `decay.rs`      |
| 22  | 21  | `diagnose_missed_violation` | `diagnostic.rs` |
| 20  | 40  | `decompose_basis`           | `query.rs`      |

---

## Proposals

Each proposal is independent and scoped to a single file or small group of related
files. They are organised into three tiers:

- **Tier 1** — Structural decomposition. High-impact, directly lowers the CC/Cog numbers.
- **Tier 2** — Cohesion and naming. Medium-impact; improves understandability without
  moving large code blocks.
- **Tier 3** — Static safety and polish. Lower-risk fixes; correctness / long-term
  sustainability.

---

## Tier 1 — Structural Decomposition

### P1 — `evict.rs`: Decompose `evict_tip` into named phases

**File**: `src/graph/algorithm/evict.rs`
**Metric impact**: `evict_tip` CC=32, Cog=37, ~220 SLOC, carries `#[allow(clippy::too_many_lines)]`

**Problem**: `evict_tip` combines five distinct, sequential concerns in a single body:

1. Capture parent state snapshot (before dealloc)
2. G-sum absorption into parent
3. V-leaf removal and V-tree cleanup
4. Flag propagation (intensity, evictable, exposed)
5. Violation queue decision (which push function to call)

Steps 1–5 are sequential and do not interleave, which means they are prime candidates
for extraction into named private helpers. The existing `parent_snapshot` raw tuple
(`parent_state_after`, `parent_lo`, `parent_hi`) is an unnamed intermediate
data-transfer object.

**Proposal**:

- Define `struct ParentSnapshot { state: GState, lo: C, hi: C }` to replace the raw
  tuple passed from `evict_tip` to `plateau_after_evict`.
- Extract the violation-queue decision block into a private `fn push_eviction_violations`.
  Input: the V-parent state after removal (child count, `change_point`, `collapse_sibling`).
  This block currently decodes a nested `v_parent.map_or(...)` expression whose output is
  then matched against three cases — a named function with a clear signature will make the
  branching logic readable.
- (Optional, lower priority) Extract the G-sum absorption step into
  `fn absorb_child_sum_into_parent(gnodes, parent_id, evicted_own)` — pure mutation with
  no side effects; easy to unit-test.

**Expected outcome**: `evict_tip` drops from ~220 to ~80 lines; each extracted function
is independently understandable and testable.

---

### P2 — `evict.rs`: Move `plateau_after_evict` to `plateau.rs`

**File**: `src/graph/algorithm/evict.rs` → `src/graph/algorithm/plateau.rs`
**Metric impact**: Removes ~150 LOC from `evict.rs`; `plateau.rs` already hosts all other
`plateau_after_*` hooks.

**Problem**: `plateau_after_evict` is the only `plateau_after_*` function not in
`plateau.rs`. This is an accidental cohesion break. The function takes six parameters
and has two structurally identical loops (`left_keys` scan, `right_keys` scan) that
both iterate over a key range and call `remove_from_basis`. The duplication suggests a
missing private helper `fn remove_basis_keys_in_range(basis, keys)`.

**Proposal**:

- Move `plateau_after_evict` (and its `left_keys`/`right_keys` loops) to `plateau.rs` as
  `pub(crate) fn plateau_after_evict(graph, ...)`.
- Extract `fn remove_basis_keys_in_range` as a private helper within `plateau.rs` to
  eliminate the two near-identical scan loops.
- Update `evict.rs` call site to use the new location (no visible behaviour change).

---

### P3 — `plateau.rs`: Extract normalisation into a `normalise` sub-module

**File**: `src/graph/algorithm/plateau.rs`
**Metric impact**: Removes ~400 LOC from `plateau.rs`

**Problem**: `plateau.rs` is 1555 lines and combines two distinct sub-systems:
(a) the plateau index itself (placement, removal, event hooks) and
(b) basin normalisation (`consolidate_basis_up`, `consolidate_all_basis`,
`normalize_plateaus`). The normaliser has its own internal state traversal logic and
reads the plateau data as a client of the index — it is not part of the index itself.

**Proposal**:

- Create `src/graph/algorithm/plateau/` directory with:
  - `mod.rs` (or keep current `plateau.rs` as entry point with `pub use`)
  - `normalise.rs` — holds `normalize_plateaus`, `consolidate_basis_up`,
    `consolidate_all_basis`, `plateau_recompute_sums`
- Alternatively, if keeping a flat structure: extract the three normalise functions into a
  `fn normalize_all(graph)` wrapper that calls them in sequence, which at minimum reduces
  the amount of code interleaved with placement logic.

---

### P4 — `plateau.rs`: Eliminate no-op stub duplication

**File**: `src/graph/algorithm/plateau.rs`
**Problem**: Every `#[cfg(feature = "dynamic-contour-tracking")]` method in `plateau.rs`
has an `#[cfg(not(feature = ...))]` no-op counterpart (`#[inline(always)] fn ... {}`).
With ~15 feature-gated methods, this produces ~30 function bodies. The no-op stubs are
not tested and add visual noise that makes the real logic harder to find.

**Proposal**: Collect all no-op stubs into a single
`#[cfg(not(feature = "dynamic-contour-tracking"))] mod noop { ... }` block at the bottom
of the file (or a dedicated `plateau_noop.rs` if the count warrants it). This keeps the
`cfg(feature)` path clean and moves the stubs to an obviously secondary location.

---

### P5 — `rebalance.rs`: Extract violation-push functions into a sub-file

**File**: `src/graph/algorithm/rebalance.rs`
**Metric impact**: Removes ~300 LOC from `rebalance.rs`

**Problem**: `rebalance.rs` hosts 8 public `push_*_violations` functions plus 2 private
helpers (`push_grandchild_violations`, `push_children_violations`), totalling ~300 lines
of push logic. These functions are a cohesive family (they all call `violations.push(...)`
based on structural conditions) but they are unrelated to the rebalancing algorithm
itself. They are also the primary clients of `ViolationSources`, which already has its own
file.

**Proposal**:

- Create `src/graph/algorithm/violation_push.rs` containing all `push_*` functions.
- `rebalance.rs` calls them via a `use super::violation_push::push_*` import.
- This also makes it easier to add tests for individual push functions
  without setting up a full rebalance scenario.

**Note**: Each push function currently exists as both a plain version and a
`*_with_config(violations, sources: ViolationSources)` variant. The plain version always
calls the config variant with `ViolationSources::all_enabled()`. After extraction, consider
whether the plain version is still needed in the public API or whether callers should be
updated to pass the config explicitly (making the sources visible at call sites).

---

### P6 — `decay.rs`: Extract `depth_attenuation_factors` and `post_mutation_repair`

**File**: `src/graph/algorithm/decay.rs`
**Metric impact**: `decay_selective` drops from ~130 to ~50 lines

**Problem**:

1. `decay_selective` has three `if att == 0.0 / att.is_infinite() / else` branches that
   each build a `factors: Vec<f64>`. Only the method of constructing the factors differs;
   the subsequent application code is the same.
2. Both `decay_uniform` and `decay_selective` end with an identical 15-line sequence:
   G-sum recompute → V-tree propagate → `find_violated_nodes` → `rebalance` →
   `plateau_recompute_sums` → `normalize_plateaus` → `repair_p_i4`. This sequence
   is duplicated verbatim.

**Proposal**:

- Extract `fn depth_attenuation_factors(att: f64, q: f64, depth_range: usize) -> Vec<f64>`
  — pure function, independently testable with property tests.
- Extract `fn post_decay_repair(graph, violations, depth_evict)` for the shared tail
  sequence (or more generically `post_mutation_repair`, anticipating future operations
  that fully rewrite node weights).
- Add a comment explaining why `att == 0.0` and `att.is_infinite()` cannot use the
  log-exp path.

---

### P7 — `query.rs`: Split sampling from range-query concerns

**File**: `src/graph/algorithm/query.rs`
**Metric impact**: `decompose_basis` CC=20, Cog=40 → reducible through helper extraction

**Problem**: `query.rs` contains two orthogonal concerns sharing an `impl GvGraph` block:
(a) random weighted `sample` (probabilistic, uses `Weighable`) and
(b) deterministic spatial queries (`get`, `range_sum`, `contour_range`).
The `decompose_basis` function handles four overlap cases plus boundary-thatch
classification in one ~90-line function, with two near-identical
`uncovered_interval` / `trimmed_interval` helpers.

**Proposal**:

- Move `sample` + `sample_child` to `src/graph/algorithm/sample.rs`.
- Extract a `fn is_boundary_thatch_overlap(node, lo, hi) -> bool` (or inline comment
  tagging) to make the classification criterion in `decompose_basis` readable.
- Make `uncovered_interval` delegate to `trimmed_interval` (or vice versa) with a
  named parameter to eliminate the structural duplication.

---

## Tier 2 — Cohesion and Naming

### P8 — `rebalance.rs`: Document the "source" violation numbering

**File**: `src/graph/algorithm/rebalance.rs`

**Problem**: Violation-push functions are named `push_source_10_violations`,
`push_side_effect_violations`, etc. Numbers 1, 2, 5 are absent; source 10 is a lonely
outlier. There are no comments or docs explaining what the numbering system represents,
where it came from, or whether the gap is intentional.

**Proposal**: Add a module-level `///` block (or inline comment on each `push_*` function)
explaining what "source N" means: the original violation taxonomy, which sources were
merged or renamed, and what triggers each. This is a documentation-only change with no
code movement.

---

### P9 — `plateau.rs`: Name and document the `p_i4` invariant

**File**: `src/graph/algorithm/plateau.rs`

**Problem**: `repair_p_i4`, `split_for_p_i4`, `find_boundary_node` all reference the
label "p_i4" without any explanation. A reader cannot understand what property is being
enforced without tracing through the entire plateau subsystem.

**Proposal**:

- Add a module-level doc comment (or a standalone `/// Invariant P-I4: ...` comment above
  `repair_p_i4`) that states: the property name in plain terms, which node types it applies
  to, when it can be violated, and what `repair_p_i4` does to restore it.
- Consider renaming to `repair_semi_internal_basis_invariant` (or a shorter descriptive
  name) so that the name carries meaning without requiring a cross-reference.

---

### P10 — `diagnostic.rs`: Rename `EvictionContext` and extract `is_ancestor`

**File**: `src/diagnostics/diagnostic.rs`

**Problem**:

- `EvictionContext` is used only by `diagnose_missed_violation`. It is an ad-hoc struct
  carrying three raw fields with no invariants. Its name is ambiguous — it sounds like a
  context for all eviction operations, not specifically for missed-violation diagnosis.
- `is_ancestor` is a generic V-tree parent-walk utility. It lives in `diagnostic.rs`
  purely by accident and is harder to find and test there.

**Proposal**:

- Rename `EvictionContext` → `MissedViolationContext` (mirrors the `diagnose_missed_violation`
  function name; clarifies that this struct is not used during eviction itself).
- Move `is_ancestor` to `src/tree/vtree.rs` or introduce a `src/tree/util.rs` containing
  small tree traversal utilities.

---

### P11 — `vtree.rs`: Clarify intensity propagation names

**File**: `src/tree/vtree.rs`

**Problem**: Three functions with similar responsibilities have confusing names:

- `propagate_v_sums(start)` — ancestor walk from a node
- `propagate_v_sums_from(node)` — recompute _this_ node's sums then walk ancestors
- `recompute_all_v_intensities(root)` — post-order recompute of the whole tree

The `_from` suffix in `propagate_v_sums_from` means "including and starting at this node",
but `from` in English typically implies a starting point that is excluded. The `_all_`
in `recompute_all_v_intensities` is a trivial wrapper with no added logic over
`recompute_v_postorder`.

**Proposal**:

- Rename `propagate_v_sums_from` → `recompute_and_propagate_v_sums` (makes the
  all-inclusive recompute explicit).
- Inline `recompute_all_v_intensities` (it is a one-line wrapper) or, if the public name
  is needed, add a doc comment explaining the difference from `propagate_v_sums`.
- Rename `update_parent_cached_intensity` → `update_parent_child_record` or
  `sync_intensity_in_parent` — the word "cached" implies a secondary store, but
  `PackedChildren.intensities` is the authoritative parent-side record.

---

### P12 — `vtree.rs`: Fix unnecessary `Vec` allocation in `recompute_v_postorder`

**File**: `src/tree/vtree.rs`

**Problem**: `recompute_v_postorder` calls `node.children()` and collects into a
`Vec<VNodeId>` _before_ checking `if node.is_entry() { return; }`. For every V-entry
node in the tree a `Vec` is allocated and immediately discarded.

**Proposal**: Move the `is_entry()` guard before the `children()` collection. This is a
small but pure-upside change (no behaviour change, avoids allocations in the common
hotpath over entry nodes).

---

## Tier 3 — Static Safety and Polish

### P13 — `coordinate.rs`: Introduce `DiscreteCoordinate` subtrait

**File**: `src/traits/coordinate.rs`

**Problem**: `next_value()` is defined on the `Coordinate` trait but implemented as a
`panic!` for `f32` and `f64`. Any code path that calls `next_value` on a float coordinate
will compile successfully but fail at runtime. Similarly, `is_final` has completely
different semantics for integers (exact unit-width check) and floats (depth-threshold
check), which is invisible at the trait level.

**Proposal**:

- Introduce `trait DiscreteCoordinate: Coordinate` implemented only by
  `u8, u16, u32, u64, u128`. Move `next_value` and the integer-variant
  `is_final` onto `DiscreteCoordinate`.
- Any function that _requires_ `next_value` can then bound on `DiscreteCoordinate`
  rather than `Coordinate`, surfacing the constraint at compile time.
- Remove or clearly document the `is_final` discrepancy between integer and float impls.

---

### P14 — `plateau.rs`: Remove `place_subtree_basis_elements` dead code

**File**: `src/graph/algorithm/plateau.rs`

**Problem**: `place_subtree_basis_elements` carries `#[allow(dead_code)]`. Dead code in a
production module accumulates over time and becomes maintenance debt that is never paid.

**Proposal**: Either delete the function (and add a note on when it might be needed in the
future) or call it from an existing algorithm step that currently re-implements its
behaviour inline. If it is preserved as intentional documentation code, move it to a
`#[cfg(test)]` block or a `/// Example` doc test so that its unused status is structural,
not suppressed.

---

### P15 — `decay.rs`: Fix confusing `let _ = is_global;`

**File**: `src/graph/algorithm/decay.rs`

**Problem**: `decay_selective` receives `is_global: bool` as a parameter and later uses
it in the parent-recompute block. After the block, there is a `let _ = is_global;` line
that appears to signal "I know this is unused" — but the variable _is_ used earlier.
The pattern is misleading; the `let _ = ...` shadowing after use implies the author was
confused, or the original intent was to remove the flag but it was only removed from part
of the function.

**Proposal**: Remove the `let _ = is_global;` line. If compiler warnings are not emitted
(because the variable is used), this is a no-op cleanup. If it was silencing a genuine
"value not used after this point" lint, add a comment instead explaining the block where
it is used.

---

### P16 — `rebalance.rs`: `legacy_promote` belongs with G-tree operations

**File**: `src/graph/algorithm/rebalance.rs`

**Problem**: `legacy_promote` is the **only** function in `rebalance.rs` that mutates
`gnodes` (the G-tree arena). Every other function is a pure V-tree operation. This
coupling forces `rebalance.rs` to carry a `GvGraph` or `gnodes` parameter even in
contexts where only V-tree access is needed.

**Proposal**: Move `legacy_promote` to `evict.rs` or introduce a
`src/graph/algorithm/promote.rs` that groups all promotion-related operations
(`standard_promote`, `skip_promote`, `legacy_promote`). This cleanly separates
V-only rebalancing from G+V joint operations.

---

## Execution Priority

| ID  | Description                                            | Impact | Risk   |
| --- | ------------------------------------------------------ | ------ | ------ |
| P1  | Decompose `evict_tip` into named phases                | High   | Low    |
| P6  | Extract `depth_attenuation_factors` + post-repair tail | High   | Low    |
| P5  | Extract violation-push functions to own file           | High   | Low    |
| P2  | Move `plateau_after_evict` to `plateau.rs`             | Medium | Low    |
| P7  | Split `sample` from range-query in `query.rs`          | Medium | Low    |
| P3  | Extract normalisation group from `plateau.rs`          | High   | Medium |
| P4  | Eliminate no-op stub duplication in `plateau.rs`       | Medium | Low    |
| P10 | Rename `EvictionContext`; move `is_ancestor`           | Low    | Low    |
| P8  | Document violation source numbering                    | Low    | None   |
| P9  | Document and name `p_i4` invariant                     | Low    | None   |
| P11 | Clarify intensity propagation names in `vtree.rs`      | Low    | Low    |
| P12 | Fix unnecessary `Vec` alloc in `recompute_v_postorder` | Low    | None   |
| P13 | Introduce `DiscreteCoordinate` subtrait                | Medium | Medium |
| P14 | Remove dead `place_subtree_basis_elements`             | Low    | None   |
| P15 | Remove misleading `let _ = is_global;`                 | Low    | None   |
| P16 | Move `legacy_promote` to a promote module              | Medium | Low    |

---

## Tracking

| ID  | Status      | Notes                                                                          |
| --- | ----------- | ------------------------------------------------------------------------------ |
| P1  | Done        | commit `731efed` — `LeafRemovalContext`, `ParentSnapshot<C>`, phase comments   |
| P2  | Done        | commit `731efed` — `plateau_after_evict` moved to `plateau/mod.rs`             |
| P3  | Done        | commit `6de0eeb` — `normalise.rs` sub-module extracted                         |
| P4  | Done        | commit `f229326` — no-op stubs collected in `plateau/noop.rs`                  |
| P5  | Done        | commit `845ac7a` — `push_*` helpers extracted to `violation_push.rs`           |
| P6  | Done        | commit `9b01720` — `post_decay_repair` + `depth_attenuation_factors` extracted |
| P7  | Done        | commit `2a5845e` — `sample`/`sample_child` moved to `algorithm/sample.rs`      |
| P8  | Not started |                                                                                |
| P9  | Not started |                                                                                |
| P10 | Not started |                                                                                |
| P11 | Not started |                                                                                |
| P12 | Not started |                                                                                |
| P13 | Not started |                                                                                |
| P14 | Not started |                                                                                |
| P15 | Done        | removed as part of P6 (`let _ = is_global` eliminated)                         |
| P16 | Not started |                                                                                |
