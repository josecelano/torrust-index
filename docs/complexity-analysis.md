# Cyclomatic Complexity Analysis

## Goal

One-time cyclomatic complexity analysis of the `src/` directory using
[`rust-code-analysis-cli`](https://github.com/mozilla/rust-code-analysis) by Mozilla.

## Tool

**`rust-code-analysis-cli`** — computes 16 code metrics per file and per
function, including:

| Metric      | Description                                        |
| ----------- | -------------------------------------------------- |
| `cc`        | Cyclomatic Complexity (McCabe)                     |
| `cognitive` | Cognitive Complexity                               |
| `nom`       | Number of functions / closures                     |
| `nargs`     | Function argument count                            |
| `nexits`    | Number of exit points                              |
| `halstead`  | Halstead suite (effort, difficulty, bugs estimate) |
| `mi`        | Maintainability Index                              |

## Steps

### 1. Install

`cargo install rust-code-analysis-cli` fails on current Rust nightly (1.96) due to
a type-check regression in the crate. Use the pre-built Linux binary instead:

```bash
curl -L -o /tmp/rca.tar.gz \
  https://github.com/mozilla/rust-code-analysis/releases/download/v0.0.25/rust-code-analysis-linux-cli-x86_64.tar.gz
tar xzf /tmp/rca.tar.gz -C /tmp
cp /tmp/rust-code-analysis-cli ~/.local/bin/
rust-code-analysis-cli --version   # should print 0.0.25
```

### 2. Quick Console Dump

Human-readable overview, all metrics printed to stdout:

```bash
rust-code-analysis-cli -m -p src/
```

### 3. Full JSON Export

One JSON file per source file written to `./metrics-output/`:

```bash
mkdir -p metrics-output
rust-code-analysis-cli -m -p src/ -O json --pr -o metrics-output/
```

### 4. Extract High-Complexity Functions

Re-runnable `jq` script that walks the nested `spaces` tree and emits every
function unit sorted by CC descending:

```bash
find metrics-output -name "*.json" | xargs -I{} sh -c '
  FILE={}
  jq --arg f "$FILE" "
    def short(f): f | ltrimstr(\"metrics-output/\") | rtrimstr(\".json\");
    def walk_s:
      if type == \"object\" and has(\"name\") and has(\"metrics\") and has(\"kind\") then
        (if .kind == \"function\"
         then {file: (short(\$f)), name: .name,
               cc: (.metrics.cyclomatic.sum // 0),
               cognitive: (.metrics.cognitive.sum // 0),
               sloc: (.metrics.loc.sloc // 0)}
         else empty end),
        (if has(\"spaces\") then (.spaces // [])[] | walk_s else empty end)
      elif type == \"object\" then (to_entries[].value | walk_s)
      else empty end;
    walk_s
  " "\$FILE"
' | jq -s "sort_by(-.cc) | .[] | select(.cc > 10)" \
  | jq -r '"CC=\(.cc)  Cog=\(.cognitive)  SLOC=\(.sloc)  \(.name)  [\(.file)]"'
```

## Results (March 2026 — pre-Q-series)

> This snapshot was taken before the P-series and Q-series refactors.
> See [Results (March 2026 — post-Q-series)](#results-march-2026--post-q-series)
> for the current baseline.

### Files by Aggregate Cyclomatic Complexity

| CC  | Cognitive | SLOC | File                               |
| --- | --------- | ---- | ---------------------------------- |
| 329 | 418       | 1801 | `src/diagnostics/invariants.rs`    |
| 241 | 215       | 1452 | `src/graph/algorithm/rebalance.rs` |
| 238 | 287       | 1554 | `src/graph/algorithm/plateau.rs`   |
| 101 | 73        | 673  | `src/graph/algorithm/query.rs`     |
| 91  | 143       | 600  | `src/graph/algorithm/evict.rs`     |
| 89  | 82        | 591  | `src/diagnostics/diagnostic.rs`    |
| 70  | 51        | 527  | `src/tree/vtree.rs`                |
| 67  | 0         | 518  | `src/traits/coordinate.rs`         |
| 66  | 12        | 617  | `src/graph/mod.rs`                 |
| 65  | 16        | 802  | `src/spatial/pewei.rs`             |
| 63  | 64        | 416  | `src/graph/algorithm/decay.rs`     |
| 58  | 8         | 390  | `src/spatial/plateau.rs`           |
| 49  | 4         | 403  | `src/nodes/vnode.rs`               |
| 44  | 7         | 330  | `src/arena.rs`                     |
| 41  | 22        | 302  | `src/graph/algorithm/extract.rs`   |
| 34  | 36        | 265  | `src/graph/algorithm/budget.rs`    |
| 32  | 27        | 223  | `src/graph/algorithm/observe.rs`   |
| 32  | 2         | 327  | `src/spatial/view.rs`              |
| 31  | 19        | 413  | `src/graph/algorithm/split.rs`     |

> File-level CC is the **sum** of all functions / impls in that file.

### Functions with CC > 10 (sorted)

| CC  | Cognitive | SLOC | Function                                  | File                           |
| --- | --------- | ---- | ----------------------------------------- | ------------------------------ |
| 39  | **95**    | 196  | `plateau_after_evict`                     | `graph/algorithm/evict.rs`     |
| 32  | 37        | 246  | `evict_tip`                               | `graph/algorithm/evict.rs`     |
| 26  | 46        | 153  | `normalize_plateaus`                      | `graph/algorithm/plateau.rs`   |
| 25  | 42        | 128  | `decay_selective`                         | `graph/algorithm/decay.rs`     |
| 24  | 16        | 115  | `dump_plateaus`                           | `diagnostics/invariants.rs`    |
| 22  | 21        | 139  | `diagnose_missed_violation`               | `diagnostics/diagnostic.rs`    |
| 20  | 40        | 90   | `decompose_basis`                         | `graph/algorithm/query.rs`     |
| 19  | 25        | 109  | `observe`                                 | `graph/algorithm/observe.rs`   |
| 17  | **49**    | 70   | `check_parent_link_consistency`           | `diagnostics/invariants.rs`    |
| 17  | 39        | 76   | `build_plateaus`                          | `graph/algorithm/plateau.rs`   |
| 17  | 11        | 128  | `place_basis_element`                     | `graph/algorithm/plateau.rs`   |
| 16  | 28        | 85   | `evict_candidates`                        | `graph/algorithm/budget.rs`    |
| 16  | 17        | 68   | `escalate_after_promote`                  | `graph/algorithm/rebalance.rs` |
| 15  | 17        | 88   | `fixup_plateau`                           | `graph/algorithm/plateau.rs`   |
| 14  | 26        | 100  | `resolve`                                 | `graph/algorithm/rebalance.rs` |
| 14  | 25        | 103  | `rebalance`                               | `graph/algorithm/rebalance.rs` |
| 13  | 19        | 46   | `contour_steps`                           | `diagnostics/invariants.rs`    |
| 13  | 15        | 81   | `dump_gtree_dot`                          | `diagnostics/invariants.rs`    |
| 13  | 13        | 57   | `check_p_i1_i_keys_are_contour_steps`     | `diagnostics/invariants.rs`    |
| 12  | 23        | 108  | `debug_assert_plateau_mirror_consistency` | `graph/algorithm/plateau.rs`   |
| 12  | 25        | 66   | `check_p_i1_iii_run_contains_tile`        | `diagnostics/invariants.rs`    |
| 12  | 22        | 65   | `check_p_i1_ii_tile_contiguity`           | `diagnostics/invariants.rs`    |
| 12  | 19        | 42   | `check_p_i5_thatch_depth`                 | `diagnostics/invariants.rs`    |
| 12  | 15        | 83   | `skip_promote`                            | `graph/algorithm/rebalance.rs` |
| 12  | 13        | 50   | `check_plateau_basis_consistency`         | `diagnostics/invariants.rs`    |

---

## Results (March 2026 — post-Q-series)

> Taken after the Q-series refactors (Q1–Q12). The codebase was also
> significantly restructured between the P and Q series: `diagnostics/invariants.rs`
> was split into several sub-files, `plateau.rs` was reorganised into a
> `plateau/` directory module, and display helpers were extracted from
> `rebalance.rs` into `algorithm/fmt.rs`.
> See [Results (March 2026 — post-R-series)](#results-march-2026--post-r-series)
> for the current baseline.

### Files by Aggregate Cyclomatic Complexity

| CC  | Cognitive | SLOC | File                                       | Δ vs pre-Q      |
| --- | --------- | ---- | ------------------------------------------ | --------------- |
| 216 | 306       | 1366 | `src/graph/algorithm/plateau/mod.rs`       | was 238 (-9 %)  |
| 127 | 185       | 618  | `src/diagnostics/plateau_invariants.rs`    | new file        |
| 115 | 173       | 614  | `src/diagnostics/invariants.rs`            | was 329 (-65 %) |
| 111 | 111       | 745  | `src/graph/algorithm/rebalance.rs`         | was 241 (-54 %) |
| 90  | 68        | 616  | `src/graph/algorithm/query.rs`             | was 101 (-11 %) |
| 72  | 52        | 591  | `src/tree/vtree.rs`                        | ≈ unchanged     |
| 67  | 52        | 378  | `src/graph/algorithm/violation_push.rs`    | —               |
| 65  | 0         | 510  | `src/traits/coordinate.rs`                 | ≈ unchanged     |
| 64  | 11        | 608  | `src/graph/gv_graph.rs`                    | —               |
| 62  | 61        | 431  | `src/graph/algorithm/decay.rs`             | ≈ unchanged     |
| 59  | 15        | 781  | `src/spatial/pewei.rs`                     | ≈ unchanged     |
| 54  | 48        | 444  | `src/graph/algorithm/evict.rs`             | was 91 (-41 %)  |
| 52  | 30        | 249  | `src/diagnostics/dump.rs`                  | new file        |
| 52  | 42        | 346  | `src/diagnostics/diagnostic.rs`            | was 89 (-42 %)  |
| 49  | 4         | 403  | `src/nodes/vnode.rs`                       | ≈ unchanged     |
| 47  | 66        | 350  | `src/graph/algorithm/plateau/normalise.rs` | new file        |
| 44  | 7         | 330  | `src/arena.rs`                             | ≈ unchanged     |
| 41  | 22        | 302  | `src/graph/algorithm/extract.rs`           | ≈ unchanged     |
| 37  | 40        | 335  | `src/graph/algorithm/promote.rs`           | —               |
| 34  | 36        | 265  | `src/graph/algorithm/budget.rs`            | ≈ unchanged     |
| 32  | 27        | 231  | `src/graph/algorithm/observe.rs`           | ≈ unchanged     |
| 31  | 17        | 418  | `src/graph/algorithm/split.rs`             | ≈ unchanged     |

> File-level CC is the **sum** of all functions / impls in that file.

### Functions with CC > 10 (sorted, post-Q-series)

| CC  | Cognitive | SLOC | Function                                   | File                                   |
| --- | --------- | ---- | ------------------------------------------ | -------------------------------------- |
| 39  | **95**    | 196  | `plateau_after_evict`                      | `graph/algorithm/plateau/mod.rs`       |
| 26  | 46        | 153  | `normalize_plateaus`                       | `graph/algorithm/plateau/normalise.rs` |
| 24  | 16        | 115  | `dump_plateaus`                            | `diagnostics/dump.rs`                  |
| 22  | 21        | 139  | `diagnose_missed_violation`                | `diagnostics/diagnostic.rs`            |
| 20  | 40        | 90   | `decompose_basis`                          | `graph/algorithm/query.rs`             |
| 19  | 25        | 117  | `observe`                                  | `graph/algorithm/observe.rs`           |
| 17  | **49**    | 70   | `check_parent_link_consistency`            | `diagnostics/invariants.rs`            |
| 17  | 39        | 76   | `build_plateaus`                           | `graph/algorithm/plateau/mod.rs`       |
| 17  | 11        | 128  | `place_basis_element`                      | `graph/algorithm/plateau/mod.rs`       |
| 16  | 28        | 194  | `evict_tip`                                | `graph/algorithm/evict.rs`             |
| 16  | 28        | 85   | `evict_candidates`                         | `graph/algorithm/budget.rs`            |
| 16  | 17        | 68   | `escalate_after_promote`                   | `graph/algorithm/rebalance.rs`         |
| 15  | 17        | 88   | `fixup_plateau`                            | `graph/algorithm/plateau/mod.rs`       |
| 14  | 26        | 102  | `resolve`                                  | `graph/algorithm/rebalance.rs`         |
| 13  | 20        | 74   | `rebalance`                                | `graph/algorithm/rebalance.rs`         |
| 13  | 19        | 44   | `contour_steps`                            | `diagnostics/plateau_invariants.rs`    |
| 13  | 15        | 81   | `dump_gtree_dot`                           | `diagnostics/dot.rs`                   |
| 13  | 13        | 57   | `check_p_i1_i_keys_are_contour_steps`      | `diagnostics/plateau_invariants.rs`    |
| 12  | 24        | 45   | `depth_attenuation_factors`                | `graph/algorithm/decay.rs`             |
| 12  | 23        | 108  | `debug_assert_plateau_mirror_consistency`  | `graph/algorithm/plateau/mod.rs`       |
| 12  | 15        | 83   | `skip_promote`                             | `graph/algorithm/promote.rs`           |
| 12  | 13        | 54   | `check_plateau_basis_consistency`          | `diagnostics/plateau_invariants.rs`    |
| 11  | 31        | 97   | `plateau_after_catalytic_split`            | `graph/algorithm/plateau/mod.rs`       |
| 11  | 28        | 55   | `audit_plateau_consistency`                | `diagnostics/plateau_audit.rs`         |
| 11  | 18        | 39   | `push_leaf_removal_violations_with_config` | `graph/algorithm/violation_push.rs`    |

> **Cognitive > cyclomatic** outliers: `plateau_after_evict` (Cog=95 vs CC=39)
> and `check_parent_link_consistency` (Cog=49 vs CC=17) indicate deep nesting
> that the McCabe number underestimates.

### Key Observations (post-Q-series)

- **`plateau/mod.rs`** is the dominant hotspot at CC=216 / Cog=306.
  `plateau_after_evict` (CC=39, Cog=95) is the single most complex function
  in the codebase and the primary Round-3 target.
- **`plateau/normalise.rs`**: `normalize_plateaus` (CC=26, Cog=46) is the second
  hardest function; it is already extracted into its own file.
- **`diagnostics/invariants.rs`** fell from CC=329 to CC=115 through file
  splitting and Q9 grouping — but `check_parent_link_consistency` (Cog=49)
  remains disproportionately hard to read due to four levels of nesting.
- **`rebalance.rs`** fell from CC=241 to CC=111; its remaining functions
  (`escalate_after_promote`, `resolve`, `rebalance`) are still > CC=13.
- **`evict.rs`** fell from CC=91 to CC=54 after `plateau_after_evict` was moved
  out; `evict_tip` (CC=16) and `evict_candidates` (CC=16) remain.
- Overall the algorithm core is about half as complex as the pre-P baseline,
  with the plateau subsystem now being the sole high-complexity concentration.

---

## Results (March 2026 — post-R-series)

> Taken after the R-series refactors (R1–R8, commit `5335fa8`).
> The R-series focused on **readability** — phase banners, match-arm labels, and
> branch comments — rather than structural decomposition. The one structural
> change (R3) extracted `check_g_parent_links` and `check_v_parent_links` from
> `check_parent_link_consistency`, removing the highest cognitive-vs-cyclomatic
> outlier (CC=17, Cog=**49**) from the high-complexity function list.

### Files by Aggregate Cyclomatic Complexity

| CC  | Cognitive | SLOC | File                                       | Δ vs post-Q          |
| --- | --------- | ---- | ------------------------------------------ | -------------------- |
| 216 | 306       | 1389 | `src/graph/algorithm/plateau/mod.rs`       | SLOC +23 (banners)   |
| 127 | 185       | 618  | `src/diagnostics/plateau_invariants.rs`    | ≈ unchanged          |
| 117 | 173       | 631  | `src/diagnostics/invariants.rs`            | was 115 (+2 from R3) |
| 111 | 111       | 749  | `src/graph/algorithm/rebalance.rs`         | ≈ unchanged          |
| 90  | 68        | 616  | `src/graph/algorithm/query.rs`             | ≈ unchanged          |
| 72  | 52        | 591  | `src/tree/vtree.rs`                        | ≈ unchanged          |
| 67  | 52        | 378  | `src/graph/algorithm/violation_push.rs`    | ≈ unchanged          |
| 65  | 0         | 510  | `src/traits/coordinate.rs`                 | ≈ unchanged          |
| 64  | 11        | 608  | `src/graph/gv_graph.rs`                    | ≈ unchanged          |
| 62  | 61        | 434  | `src/graph/algorithm/decay.rs`             | ≈ unchanged          |
| 59  | 15        | 781  | `src/spatial/pewei.rs`                     | ≈ unchanged          |
| 54  | 48        | 444  | `src/graph/algorithm/evict.rs`             | ≈ unchanged          |
| 52  | 42        | 346  | `src/diagnostics/diagnostic.rs`            | ≈ unchanged          |
| 52  | 30        | 249  | `src/diagnostics/dump.rs`                  | ≈ unchanged          |
| 49  | 4         | 403  | `src/nodes/vnode.rs`                       | ≈ unchanged          |
| 47  | 66        | 354  | `src/graph/algorithm/plateau/normalise.rs` | ≈ unchanged          |
| 44  | 7         | 330  | `src/arena.rs`                             | ≈ unchanged          |
| 41  | 22        | 302  | `src/graph/algorithm/extract.rs`           | ≈ unchanged          |
| 37  | 40        | 335  | `src/graph/algorithm/promote.rs`           | ≈ unchanged          |
| 34  | 36        | 267  | `src/graph/algorithm/budget.rs`            | ≈ unchanged          |
| 32  | 27        | 231  | `src/graph/algorithm/observe.rs`           | ≈ unchanged          |
| 31  | 17        | 418  | `src/graph/algorithm/split.rs`             | ≈ unchanged          |

> File-level CC is the **sum** of all functions / impls in that file.
> `invariants.rs` gained +2 CC because R3 added two new helper function entry
> points (`check_g_parent_links`, `check_v_parent_links`), each counted as CC=1.

### Functions with CC > 10 (sorted, post-R-series)

| CC  | Cognitive | SLOC | Function                                   | File                                   |
| --- | --------- | ---- | ------------------------------------------ | -------------------------------------- |
| 39  | **95**    | 203  | `plateau_after_evict`                      | `graph/algorithm/plateau/mod.rs`       |
| 26  | 46        | 157  | `normalize_plateaus`                       | `graph/algorithm/plateau/normalise.rs` |
| 24  | 16        | 115  | `dump_plateaus`                            | `diagnostics/dump.rs`                  |
| 22  | 21        | 139  | `diagnose_missed_violation`                | `diagnostics/diagnostic.rs`            |
| 20  | 40        | 90   | `decompose_basis`                          | `graph/algorithm/query.rs`             |
| 19  | 25        | 117  | `observe`                                  | `graph/algorithm/observe.rs`           |
| 17  | 39        | 76   | `build_plateaus`                           | `graph/algorithm/plateau/mod.rs`       |
| 17  | 11        | 139  | `place_basis_element`                      | `graph/algorithm/plateau/mod.rs`       |
| 16  | 28        | 87   | `evict_candidates`                         | `graph/algorithm/budget.rs`            |
| 16  | 16        | 194  | `evict_tip`                                | `graph/algorithm/evict.rs`             |
| 16  | 17        | 72   | `escalate_after_promote`                   | `graph/algorithm/rebalance.rs`         |
| 15  | 17        | 88   | `fixup_plateau`                            | `graph/algorithm/plateau/mod.rs`       |
| 14  | 26        | 102  | `resolve`                                  | `graph/algorithm/rebalance.rs`         |
| 13  | 20        | 74   | `rebalance`                                | `graph/algorithm/rebalance.rs`         |
| 13  | 19        | 44   | `contour_steps`                            | `diagnostics/plateau_invariants.rs`    |
| 13  | 15        | 81   | `dump_gtree_dot`                           | `diagnostics/dot.rs`                   |
| 13  | 13        | 57   | `check_p_i1_i_keys_are_contour_steps`      | `diagnostics/plateau_invariants.rs`    |
| 12  | 24        | 48   | `depth_attenuation_factors`                | `graph/algorithm/decay.rs`             |
| 12  | 23        | 108  | `debug_assert_plateau_mirror_consistency`  | `graph/algorithm/plateau/mod.rs`       |
| 12  | 22        | 69   | `check_p_i1_ii_tile_contiguity`            | `diagnostics/plateau_invariants.rs`    |
| 12  | 25        | 70   | `check_p_i1_iii_run_contains_tile`         | `diagnostics/plateau_invariants.rs`    |
| 12  | 19        | 42   | `check_p_i5_thatch_depth`                  | `diagnostics/plateau_invariants.rs`    |
| 12  | 15        | 83   | `skip_promote`                             | `graph/algorithm/promote.rs`           |
| 12  | 13        | 54   | `check_plateau_basis_consistency`          | `diagnostics/plateau_invariants.rs`    |
| 11  | **31**    | 102  | `plateau_after_catalytic_split`            | `graph/algorithm/plateau/mod.rs`       |
| 11  | 28        | 55   | `audit_plateau_consistency`                | `diagnostics/plateau_audit.rs`         |
| 11  | 18        | 39   | `push_leaf_removal_violations_with_config` | `graph/algorithm/violation_push.rs`    |
| 11  | 17        | 51   | `repair_p_i4`                              | `graph/algorithm/plateau/mod.rs`       |

> **Δ vs post-Q**: `check_parent_link_consistency` (CC=17, Cog=**49**) is no
> longer in the high-complexity list — it was reduced to a 2-line delegator by
> R3. All other CC values are unchanged; the R-series added structural comments
> only.

### Key Observations (post-R-series)

- **`plateau/mod.rs`** remains the dominant hotspot (CC=216, Cog=306).
  `plateau_after_evict` (CC=39, Cog=95) is still the hardest single function;
  further reduction would require logic extraction, not just annotation.
- **`check_parent_link_consistency`** was eliminated from the high-complexity
  function list by R3: its CC=17, Cog=49 body is now split across two focused
  helpers (`check_g_parent_links`, `check_v_parent_links`), each with CC ≤ 9.
- **SLOC increases** are cosmetic: ~4–25 lines per file from inserted phase
  banners, arm comments, and branch labels.
- **No regressions**: all 418 tests pass after the R-series.
- **Next targets** (if a Round 4 is planned): `plateau_after_evict` (Cog=95),
  `normalize_plateaus` (Cog=46), `plateau_after_catalytic_split` (Cog=31), and
  `decompose_basis` (Cog=40) are the remaining cognitive-complexity outliers.

## Thresholds (Reference)

| CC Range | Risk Level                            |
| -------- | ------------------------------------- |
| 1–5      | Low — simple, easy to test            |
| 6–10     | Moderate — manageable                 |
| 11–20    | High — consider refactoring           |
| > 20     | Very High — hard to test and maintain |

## Scope

- **Included**: all `.rs` files under `src/`
- **Excluded**: CI integration, automated enforcement, test files under `tests/`
