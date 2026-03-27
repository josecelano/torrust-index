# Complexity Reduction Plan — Round 3

> Archived predecessors:
>
> - [`docs/archive/complexity-reduction-plan.md`](archive/complexity-reduction-plan.md) — P-series (16 items, all done)
> - [`docs/archive/complexity-reduction-plan-2.md`](archive/complexity-reduction-plan-2.md) — Q-series (12 items, all done)
>
> This document records the remaining complexity found during a fresh codebase
> sweep after completing all Q-series items.

---

## How to read this plan

Items are numbered **R1–R8** (R for _readability_, to distinguish from the
P- and Q-series). Each item has:

- **File / line estimate** — primary location
- **Problem** — concrete description of what makes the code hard to read
- **Proposal** — small, testable change with clear success criteria
- **Effort** — LoC added or reorganised, risk of correctness change
- **Impact** — readability / discoverability gain

---

## Summary of Round-3 targets

| ID  | File                                   | Hardest function                | CC  | Cog |
| --- | -------------------------------------- | ------------------------------- | --- | --- |
| R1  | `graph/algorithm/plateau/mod.rs`       | `plateau_after_evict`           | 39  | 95  |
| R2  | `graph/algorithm/plateau/normalise.rs` | `normalize_plateaus`            | 26  | 46  |
| R3  | `diagnostics/invariants.rs`            | `check_parent_link_consistency` | 17  | 49  |
| R4  | `graph/algorithm/rebalance.rs`         | `escalate_after_promote`        | 16  | 17  |
| R5  | `graph/algorithm/plateau/mod.rs`       | `place_basis_element`           | 17  | 11  |
| R6  | `graph/algorithm/budget.rs`            | `evict_candidates`              | 16  | 28  |
| R7  | `graph/algorithm/plateau/mod.rs`       | `plateau_after_catalytic_split` | 11  | 31  |
| R8  | `graph/algorithm/decay.rs`             | `depth_attenuation_factors`     | 12  | 24  |

---

## Items

### R1 — `plateau/mod.rs`: Add phase banners to `plateau_after_evict`

**File**: `src/graph/algorithm/plateau/mod.rs` line 1013

**Problem**: `plateau_after_evict` is CC=39, Cog=**95** — the single most
complex function in the codebase. Its 196 SLOC implement 6 distinct
algorithmic phases with no structural separation, making it difficult to
identify which section handles which concern. The high cognitive score
(95 vs CC=39) reflects deeply nested control flow across all phases.

The 6 phases are:

1. Remove the evicted G-node from the plateau basis, then locate the
   covering plateau key — either by removing the parent directly, or by
   walking the ancestor chain.
2. Collect same-plateau siblings displaced by the parent state change.
3. Fixup the `evicted_key` and `ancestor_key` plateau entries.
4. Compute the parent's new depth from its post-eviction interval.
5. Evacuate right-adjacent and left-adjacent same-depth plateaus.
6. Collect all displaced nodes into `to_place` and call `place_sorted`.

**Proposal**: Add 6 `// ── Phase N: … ──` banners at the start of each
phase. No code changes.

**Effort**: ~6 lines added. Zero correctness risk.

**Impact**: Cog score expected to fall proportionally; reader can jump
directly to a phase without reading the whole function.

---

### R2 — `plateau/normalise.rs`: Add phase banners to `normalize_plateaus`

**File**: `src/graph/algorithm/plateau/normalise.rs` (line ~140)

**Problem**: `normalize_plateaus` is CC=26, Cog=46. The function already
has `debug_assert` checkpoints labelled `"normalize step 1..4"`, but the
production code has no corresponding structural markers. A reader cannot
quickly locate, for example, where the sweep-merge loop begins.

The 4 phases are:

1. Collect and expand basis elements (DFS walk per basis node).
2. Sort the collected elements by basis-edge key.
3. Sweep-and-merge into the new plateau map (`new_plateaus`), building the
   `assignments` vector in parallel.
4. Install `new_plateaus`, rebuild the basis → `consolidate_all_basis`.

**Proposal**: Add 4 `// ── Phase N: … ──` banners aligned with the existing
step comments. No code changes.

**Effort**: ~4 lines added. Zero correctness risk.

**Impact**: Cog score expected to drop; function flow self-documents.

---

### R3 — `diagnostics/invariants.rs`: Extract helpers from `check_parent_link_consistency`

**File**: `src/diagnostics/invariants.rs` line ~448

**Problem**: `check_parent_link_consistency` is CC=17, Cog=**49** — the
worst cognitive-vs-CC ratio in the codebase. The function runs 4
independent checks, each with 3–4 levels of nesting, consecutively, in a
single 70-SLOC body:

1. G-left child parent back-link
2. G-right child parent back-link
3. V-structural child parent back-link
4. V-node's claimed parent contains the node in its child list

Each group contains an `iter_occupied` loop → `if let Some` → occupied
check → mismatch check. The nesting is deep but repetitive.

**Proposal**: Extract two private helpers:

- `fn check_g_parent_links(graph, errors)` — covers items 1 & 2 (G-tree).
- `fn check_v_parent_links(graph, errors)` — covers items 3 & 4 (V-tree).

`check_parent_link_consistency` becomes a 4-line delegator. Each helper
has a `///` doc comment stating which invariant it checks.

**Effort**: ~20 lines of structural change. The logic is moved verbatim
so correctness risk is minimal; verified by existing tests.

**Impact**: Cog per function drops from 49 to ~20 each. The extracted
helpers are independently readable.

---

### R4 — `rebalance.rs`: Add phase banners to `escalate_after_promote`

**File**: `src/graph/algorithm/rebalance.rs` line ~159

**Problem**: `escalate_after_promote` is CC=16, Cog=17. The function has
3 early-return guards followed by a 4-phase main body, but the phases are
not labelled. The function is called from `resolve()` and its structure
is easy to misread.

The 4 phases are:

1. Find the heaviest child; early return if neither it nor its subtree is
   violated.
2. Contract the 2-child parent `p`; propagate side-effect violations.
   Return if the violation is resolved.
3. Optionally contract the grandparent `g` if it has 3 children. Return
   if the violation is resolved.
4. Skip-promote fallback: push grandparent violations and propagate.

**Proposal**: Add 4 `// ── Phase N: … ──` banners. No code changes.

**Effort**: ~4 lines added. Zero correctness risk.

**Impact**: A reader tracing `resolve()` can locate the escalation phase
that fired without reading 68 SLOC.

---

### R5 — `plateau/mod.rs`: Label match arms in `place_basis_element`

**File**: `src/graph/algorithm/plateau/mod.rs` line ~300

**Problem**: `place_basis_element` is CC=17, Cog=11. Its body is a single
large `match (left_key, right_key)` with 4 arms. The tracing calls at the
end of each arm name the strategy (`merge-both`, `insert-left`,
`rekey-right`, `new-plateau`), but the opening of each arm is unlabelled.
The two inner loops in `Some(lk), Some(rk)` and `None, Some(rk)` that
re-key basis elements are the hardest parts to understand.

**Proposal**: Add one `// ── … ──` banner comment at the start of each
match arm (4 banners), naming the strategy and briefly describing the
re-key operation where applicable. No code changes.

**Effort**: ~4 lines added. Zero correctness risk.

**Impact**: A reader can scan the match arms without tracing each arm body
to understand what it does.

---

### R6 — `budget.rs`: Add phase banners to `evict_candidates`

**File**: `src/graph/algorithm/budget.rs` line ~84

**Problem**: `evict_candidates` is CC=16, Cog=28. The function has two
clearly separated concerns — (1) the per-candidate eviction loop with its
filtering logic and (2) post-batch rebalance + plateau repair — but there
are no structural markers separating them.

The 2 phases are:

1. Per-candidate filtering loop: skip dead / non-evictable / too-shallow
   nodes; call `evict_tip`; optionally stop at `stop_at` count.
2. Post-batch repair: if any evictions occurred, rebalance, handle promotes,
   run `normalize_plateaus`, then `repair_p_i4`.

**Proposal**: Add 2 `// ── Phase N: … ──` banners. No code changes.

**Effort**: ~2 lines added. Zero correctness risk.

**Impact**: Phase 2 is often the section readers skip to when debugging
post-eviction state; a banner makes it immediately findable.

---

### R7 — `plateau/mod.rs`: Add phase banners to `plateau_after_catalytic_split`

**File**: `src/graph/algorithm/plateau/mod.rs` line ~832

**Problem**: `plateau_after_catalytic_split` is CC=11, Cog=**31** — the
cognitive score is nearly 3× the cyclomatic, reflecting a complex nested
path-walk. The function has 3 phases with no visual separation.

The 3 phases are:

1. Locate the covering basis element for `g_id`: either it is in the basis
   directly, or an ancestor is found via a path-walk. Collect all displaced
   co-members.
2. Fixup the old plateau key.
3. Collect displaced subtree elements, determine whether the split is
   uniform-depth, then call `place_sorted`.

**Proposal**: Add 3 `// ── Phase N: … ──` banners. No code changes.

**Effort**: ~3 lines added. Zero correctness risk.

**Impact**: Inline path-walk (phase 1) is the hardest part; a banner makes
it clear where the walk ends and placement begins.

---

### R8 — `decay.rs`: Label branches in `depth_attenuation_factors`

**File**: `src/graph/algorithm/decay.rs` line ~31

**Problem**: `depth_attenuation_factors` is CC=12, Cog=24. The function
already has a full `///` doc comment, but its body is three consecutive
unblocked branches for the `att == 0.0`, `att.is_infinite()`, and normal
float cases. Each branch contains an inline closure with its own nesting,
and without labels a reader has to reparse the if-chain header to know
which special case they are reading.

**Proposal**: Add 3 inline `// ──` line comments naming the three branches:

- `// ── Special case: zero attenuation ──`
- `// ── Special case: infinite attenuation ──`
- `// ── Normal case: finite positive attenuation ──`

No code changes.

**Effort**: ~3 lines added. Zero correctness risk.

**Impact**: Each branch is ~15 SLOC; the labels let a reader go directly
to the relevant branch without reading the others.

---

## Tracking

| ID  | Status | Notes |
| --- | ------ | ----- |
| R1  | Done   |       |
| R2  | Done   |       |
| R3  | Done   |       |
| R4  | Done   |       |
| R5  | Done   |       |
| R6  | Done   |       |
| R7  | Done   |       |
| R8  | Done   |       |
