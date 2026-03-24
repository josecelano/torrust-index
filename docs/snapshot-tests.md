# Snapshot Tests

Snapshot tests are the primary regression guard for `GvGraph` internals.
They let you refactor freely while being instantly alerted if any observable
behaviour changes.

## How they work

Each test scenario runs a deterministic sequence of operations
(`observe`, `decay`, …) and calls `insta::assert_snapshot!` after every
meaningful step. The first time the tests run there are no reference files,
so `insta` writes `.snap` files under `tests/snapshots/` and the test fails
with "snapshot was not reviewed yet". You inspect and accept the snapshots
once, commit the `.snap` files, and from that point on every subsequent run
compares the output against those files. Any difference is a test failure.

```
tests/
  snapshot_tests.rs          ← test code
  snapshots/
    snapshot_tests__*.snap   ← committed reference files (source of truth)
```

The `.snap` files are plain text and intentionally kept in version control.
`git diff` on them shows exactly what changed between two states of the code.

## Snapshot format

Every snapshot contains three sections:

```
total_sum: <n>

G-tree:
<box-drawing tree of all allocated G-nodes>

V-tree (active nodes):
<box-drawing tree of only the live/observed G-nodes>
```

**G-tree** — every G-node that has ever been allocated, including silent
placeholder nodes whose `sum = 0`. This section captures the geometric
partition structure of the coordinate space. A change here means the
splitting or eviction logic behaved differently.

**V-tree** — only the subset of G-nodes that hold a live V-tree entry
(returned by `graph.layers()`). This section captures which nodes are
actively tracking observations and what their accumulated values are. A
change here means either the active set or the accumulated values shifted.

Each node line has the form:

```
[d<depth>] <start>..<end>  own=<own>  sum=<sum>  <state>
```

| Field        | Meaning                                                         |
| ------------ | --------------------------------------------------------------- |
| `depth`      | Depth in the G-tree (root = 0)                                  |
| `start..end` | Coordinate range covered by this node                           |
| `own`        | Value accumulated directly at this node (`terminal` leaf value) |
| `sum`        | Total value for this subtree (`own` + all descendants)          |
| `state`      | `T` = Terminal, `I` = Internal, `S` = SemiInternal              |

Example — after a split is triggered:

```
---
source: tests/snapshot_tests.rs
expression: snapshot_of(&graph)
---
total_sum: 6

G-tree:
[d0] 0..65536  own=6  sum=6  I
├── [d1] 0..32768  own=0  sum=0  T
└── [d1] 32768..65536  own=0  sum=0  T

V-tree (active nodes):
[d0] 0..65536  own=6  sum=6  I
├── [d1] 0..32768  own=0  sum=0  T
└── [d1] 32768..65536  own=0  sum=0  T
```

Example — after a decay pass (topology unchanged, values halved):

```
---
source: tests/snapshot_tests.rs
expression: snapshot_of(&graph)
---
total_sum: 13

G-tree:
[d0] 0..65536  own=3  sum=13  I
├── [d1] 0..32768  own=3  sum=10  I
│   ├── [d2] 0..16384  own=3  sum=7  I
│   │   ├── [d3] 0..8192  own=2  sum=4  I
│   │   │   ├── [d4] 0..4096  own=2  sum=2  T
│   │   │   └── [d4] 4096..8192  own=0  sum=0  T
│   │   └── [d3] 8192..16384  own=0  sum=0  T
│   └── [d2] 16384..32768  own=0  sum=0  T
└── [d1] 32768..65536  own=0  sum=0  T

V-tree (active nodes):
[d0] 0..65536  own=3  sum=13  I
└── ...
```

Notice how the G-tree and the V-tree can diverge: after observations on a
single coordinate the G-tree grows new placeholder children on every split
(shown in gray in the TUI), while the V-tree only tracks the nodes that were
actually on the observation path.

## Why step-by-step snapshots instead of just the final state

Two different code paths can produce the same `total_sum` while the internal
tree topology is completely different (different split points, different `own`
distributions). Step-by-step snapshots distinguish these cases and pinpoint
_when_ a divergence starts.

That said, scenarios are kept short on purpose — a handful of meaningful
checkpoints per scenario is enough. The goal is regression detection, not
exhaustive enumeration.

## Workflow

### Initial setup (first time, no `.snap` files yet)

Auto-accept all new snapshots without interactive review:

```bash
INSTA_UPDATE=unseen cargo test --test snapshot_tests
```

Or generate them first, then review interactively:

```bash
cargo test --test snapshot_tests      # fails; writes .snap.new files
cargo insta review                    # press `a` to accept, `r` to reject
```

Then **commit the `.snap` files** — they are the source of truth.

### Normal development

```bash
cargo test --test snapshot_tests      # passes when nothing changed
```

### After a refactoring that changes behaviour intentionally

```bash
cargo test --test snapshot_tests      # fails with inline diff
cargo insta review                    # inspect each diff carefully
                                      # press `a` to accept intended changes
                                      # press `r` to reject unintended ones
git add tests/snapshots/
git commit -m "test: update snapshots after <description>"
```

### Adding a new scenario

1. Add a `#[test]` function to `tests/snapshot_tests.rs` that calls
   `insta::assert_snapshot!("my_scenario__01_label", snapshot_of(&graph))`.
2. Run `INSTA_UPDATE=unseen cargo test --test snapshot_tests` to generate the
   `.snap` files.
3. Verify the new snapshots look correct.
4. Commit both the test code and the `.snap` files.

### Workflow summary

```
Run tests
  │
  ├─ All pass ──────────────────────────────────────────────────── ✓ done
  │
  └─ Failure (snapshot mismatch)
       │
       ├─ Unexpected change ──► fix the code, re-run tests
       │
       └─ Intended change ────► cargo insta review
                                  ├─ accept (a) ──► git add/commit .snap files
                                  └─ reject (r) ──► reverts to previous snapshot
```

## Current scenarios

| Test function              | Steps | What it exercises                                                                                      |
| -------------------------- | ----- | ------------------------------------------------------------------------------------------------------ |
| `scenario_simple_observe`  | 3     | Basic accumulation — three isolated coordinates, no splits (split threshold not crossed)               |
| `scenario_split_triggered` | 3     | Tree splitting — captures the tree just before split, after first split, and after several deep splits |
| `scenario_decay`           | 3     | Value decay — topology is identical before and after decay; only `own` / `sum` values change           |
| `scenario_two_hot_spots`   | 3     | Two independent clusters — verifies both subtrees are tracked separately and decay affects both        |

## Design decisions

**Why `N=16` in tests (not `N=32` like production)?**  
N=16 gives a 0..65535 address space. Node range labels are short and fit
comfortably in one line. With N=32 (IPv4) the ranges would be large numbers
like `0..4294967296`, making diffs harder to read at a glance.

**Why G-tree and V-tree in the same snapshot?**  
They answer different questions. The G-tree shows the spatial partition
(structure); the V-tree shows which regions have live observations (activity).
A refactoring that accidentally merges two nodes would show up in the G-tree
diff. One that resets accumulated values to zero would show up in the V-tree.
Having both in the same snapshot means one failing test gives you the full
picture.

**Why commit `.snap` files?**  
They are the specification of expected behaviour, not build artefacts. Unlike
a golden-file checked in once and forgotten, `insta` makes the review step
explicit and surfaces the diff immediately. The review step forces you to
consciously sign off on every change.
