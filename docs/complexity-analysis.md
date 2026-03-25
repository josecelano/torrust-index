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

## Results (March 2026)

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

> **Cognitive > cyclomatic** (e.g. `plateau_after_evict` Cog=95, CC=39) indicates
> deeply nested or highly branching control flow that is harder to read than the
> McCabe number alone suggests.

### Key Observations

- **`graph/algorithm/evict.rs`** contains the two hardest individual functions
  (`plateau_after_evict` CC=39/Cog=95, `evict_tip` CC=32) — primary refactoring
  candidates.
- **`graph/algorithm/plateau.rs`** and **`graph/algorithm/rebalance.rs`** are the
  largest files by aggregate CC; both exceed 200.
- **`diagnostics/invariants.rs`** has the highest aggregate CC (329) because it
  hosts many small-but-numerous invariant-check functions, not a single monolith.
- **`diagnostics/`** functions have disproportionately high cognitive complexity
  (`check_parent_link_consistency` Cog=49 at only CC=17) — deeply nested
  assertions rather than linear branching.

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
