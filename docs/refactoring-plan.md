# Refactoring Plan

This document is an ordered, incremental plan to implement the design improvements
described in [design-improvement-opportunities.md](design-improvement-opportunities.md).

## Guiding principles

- **One concern per commit.** Each step produces a passing, reviewable commit.
- **Tests are the safety net.** Run `cargo test --all-features` after every step.
  If any test fails the step is not done.
- **Compile-check early.** Use `cargo check --all-features` after mechanical
  renames before running the full suite.
- **Plan is a living document.** After each phase, re-read this plan and append
  observations under the phase's "Review checkpoint" section. New findings while
  working on one phase may reveal follow-on steps or invalidate later phases.
- **No behaviour changes.** Every step is a pure refactoring. The
  integration tests, snapshot tests, and invariant checks are the oracle.

## How to run the test suite

```bash
# Fast: unit tests only
cargo test --lib

# Full suite (what CI runs)
cargo test --all-features

# With all feature combinations
cargo test --no-default-features
cargo test --no-default-features --features serde
cargo test --all-features
```

---

## Phase 0 — Baseline (pre-conditions)

**Goal:** Record the green baseline and ensure tooling is working.

### Step 0.1 — Record baseline test counts

Run `cargo test --all-features 2>&1 | grep "test result"` and record the numbers
here so drift is visible:

| Suite       | Passed    |
| ----------- | --------- |
| lib (unit)  | _to fill_ |
| integration | _to fill_ |
| snapshot    | _to fill_ |

### Step 0.2 — Confirm clippy is clean

```bash
cargo clippy --all-features -- -D warnings
```

### Review checkpoint

> _Fill in after completing phase 0._

---

## Phase 1 — Low-risk, self-contained improvements

These steps touch one file or one concept each. They can be done in any order and
independently validated.

### Step 1.1 — `PackedChildren<V>` → `Children<V>` enum (Opportunity #6)

**Files touched:** `src/nodes/vnode.rs`, all callers in `src/graph/algorithm/`.

**What:** Replace the `PackedChildren<V> { intensities, ids, len }` struct with:

```rust
pub(crate) enum Children<V> {
    Pair  { ids: [VNodeId; 2], intensities: [V; 2] },
    Triple{ ids: [VNodeId; 3], intensities: [V; 3] },
}
```

**Steps:**

1. Add `Children<V>` alongside `PackedChildren<V>` in `vnode.rs`.
2. Add `From<PackedChildren<V>>` conversion to migrate callers one at a time.
3. Migrate `VKind::Structural { children }` field type to `Children<V>`.
4. Update all match arms in algorithm modules (rebalance, split, evict, vtree).
5. Remove `PackedChildren<V>` and the `From` conversion.
6. Remove all `assert!(index < self.len())` guards that are now unreachable.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(nodes): replace PackedChildren with Children enum`

---

### Step 1.2 — Split `plateaus()` into a separate capability trait (Opportunity #9)

**Files touched:** `src/traits/spatial_read.rs`, `src/graph/traits.rs`,
`src/diagnostics/`, any examples.

**What:** Move `plateaus()` out of `SpatialRead` into a new `PlateauRead` trait:

```rust
pub trait PlateauRead: SpatialRead {
    fn plateaus(&self) -> impl Iterator<Item = (&BasisEdge<Self::Coord>, &Plateau<Self::Coord, Self::Accum>)>;
}
```

The `Cow<BTreeMap<…>>` return type in the concrete `GvGraph` impl becomes an
internal adapter. Callers that needed `plateaus()` via `SpatialRead` now use
`PlateauRead`.

**Steps:**

1. Define `PlateauRead` in `src/traits/` (new file `plateau_read.rs`).
2. Re-export from `src/traits/mod.rs`.
3. Implement `PlateauRead` for `GvGraph` in `src/graph/traits.rs`.
4. Update all call sites (diagnostics, examples, tests) to use `PlateauRead`.
5. Remove `plateaus()` from `SpatialRead`.
6. Remove the `Cow` import from `spatial_read.rs`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(traits): extract PlateauRead from SpatialRead`

---

### Step 1.3 — Tighten visibility of internal spatial types (Opportunity #8)

**Files touched:** `src/spatial/contour_range.rs`, `src/lib.rs`.

**What:** `BasisElement<C,V>` and `ContourRange<C,V>` are scratch types used only
inside the algorithm layer. They should not be `pub`.

**Steps:**

1. Change `pub struct BasisElement` and `pub struct ContourRange` to `pub(crate)`.
2. Check if they are re-exported in `lib.rs` — if so, remove those re-exports.
3. Compile and fix any breakage (expected: none outside crate).
4. Review `Node<C,V>`: it exposes `gnode_id` (an internal handle). Add a
   `// TODO(#8): consider replacing Node<C,V> with an opaque query result` note
   as a deferred item rather than changing it now.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(spatial): tighten visibility of internal contour types`

---

### Review checkpoint — Phase 1

> _After completing all three steps, answer:_
>
> - Did the `Children<V>` enum reveal any additional match arms that were
>   previously hidden by the `len` field?
> - Does `PlateauRead` as a separate trait feel right? Should `WeightedSampler`
>   be treated similarly?
> - Are there other `pub` types that only appear internally (candidates for
>   `pub(crate)`)?

---

## Phase 2 — Decouple `Config<V>` from the value type (Opportunity #4)

**Files touched:** `src/graph/config.rs`, `src/graph/gv_graph.rs`,
`src/graph/traits.rs`, `tests/integration.rs`, examples.

**Goal:** A non-generic `StructuralConfig` struct so that depth/budget parameters
can be reasoned about without knowing `V`.

### Step 2.1 — Extract `StructuralConfig`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralConfig {
    pub depth_create: u32,
    pub depth_evict: u32,
    pub budget: Option<usize>,
    pub alpha_relax: f64,
    pub bounded_eviction: bool,
}
```

Keep `Config<V>` as a thin wrapper:

```rust
pub struct Config<V: Accumulator> {
    pub structural: StructuralConfig,
    pub split_threshold: V,
}
```

**Steps:**

1. Add `StructuralConfig` to `config.rs`.
2. Rewrite `Config<V>` to nest it.
3. Move `validate()` to operate on `StructuralConfig` plus a `split_threshold: V`
   argument (or keep it on `Config<V>` delegating to `StructuralConfig`).
4. Update all `Config { … }` construction sites.
5. Update all field accesses (e.g., `config.depth_create` → `config.structural.depth_create`).

**Validate:** `cargo test --all-features`

### Step 2.2 — Re-export and clean up

1. Re-export `StructuralConfig` from `lib.rs` if callers need it.
2. Decide whether to keep `Config<V>` as a convenience wrapper or deprecate it.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(config): extract StructuralConfig from Config<V>`

---

### Review checkpoint — Phase 2

> - Does nesting `structural: StructuralConfig` inside `Config<V>` feel ergonomic
>   for callers, or would a `From<(StructuralConfig, V)>` helper be cleaner?
> - Are there places that use `Config<V>` only to inspect structural fields?
>   Those are now candidates to accept `&StructuralConfig` directly.

---

## Phase 3 — Encapsulate node fields (Opportunity #10)

This phase is done in two independent sub-phases: G-nodes first, then V-nodes.

### Step 3.1 — Encapsulate `GNode<C, V>` fields

**Files touched:** `src/nodes/gnode.rs`, all algorithm modules, `src/tree/gtree.rs`.

**What:** Make all `GNode` fields `pub(super)` (visible within `nodes/` only) and
expose typed mutators.

**Steps:**

1. Add accessor/mutator methods to `GNode` for every field that algorithm modules
   currently access directly:
   - Getters: `lo()`, `hi()`, `sum()`, `own()`, `left()`, `right()`, `parent()`, `entry()`
   - Mutators: `set_own(v)`, `set_sum(v)`, `link_children(l, r)`, `assign_entry(vid)`,
     `clear_entry()`, `set_parent(pid)`
2. Change all fields from `pub(crate)` to `pub(super)`.
3. Fix compilation errors in algorithm modules one by one, using the new accessors.
4. Run `cargo test --all-features` after each algorithm module is migrated.

> **Tip:** Do this one algorithm file at a time, committing when each compiles.

**Commit message per file:** `refactor(gnode): use accessors in <module-name>`
**Final commit:** `refactor(gnode): make all fields private`

---

### Step 3.2 — Encapsulate `VNode<V>` fields

Same approach as Step 3.1 for `VNode`, `VKind`, and `Children<V>`.

Fields to encapsulate: `intensity`, `parent`, `cached_depth`, `kind`.

> Note: `cached_depth` will be removed in Phase 6. For now, expose it via
> `depth_hint()` / `invalidate_depth()` accessors that hide the `AtomicU32`.

**Commit message per file:** `refactor(vnode): use accessors in <module-name>`
**Final commit:** `refactor(vnode): make all fields private`

---

### Review checkpoint — Phase 3

> - Did hiding the fields expose any algorithm module that was depending on two
>   fields that logically belong together (e.g., `lo + hi` always accessed as a
>   pair)? If so, add a `interval() -> (C, C)` method.
> - Did any accessor feel forced or awkward? That may signal a method that belongs
>   on `GNode` itself (e.g., `accumulate_own(delta)` instead of `set_own(get_own() + delta)`).

---

## Phase 4 — Introduce `GTree<C, V, N>` (Opportunities #1, #2)

This is the highest-impact change. It is done in small mechanical steps so the
test suite can be run after each one.

### Step 4.1 — Define `GTree` as a struct wrapper (no logic yet)

Create `src/tree/mod.rs` (or `src/gtree/mod.rs`) with:

```rust
pub(crate) struct GTree<C: Coordinate, V: Accumulator, const N: u32> {
    pub(crate) nodes: Arena<GNode<C, V>>,
    pub(crate) root:  GNodeId,
    pub(crate) node_count:     u32,
    pub(crate) terminal_count: u32,
    pub(crate) live_depth_evict:  u32,
    pub(crate) live_depth_create: u32,
    pub(crate) depth_buffer: u32,
    pub(crate) headroom:     usize,
    pub(crate) soft_limit:   Option<usize>,
}
```

**Steps:**

1. Add the struct to the tree module. No methods yet.
2. Add a `GTree::new(config: &StructuralConfig) -> Self` constructor.
3. In `GvGraph`, replace the corresponding fields with `pub(crate) gtree: GTree<C,V,N>`.
4. Fix every access: `self.gnodes` → `self.gtree.nodes`, `self.g_root` → `self.gtree.root`, etc.
5. Run `cargo check --all-features` before running the full test suite.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): introduce GTree struct (fields only)`

---

### Step 4.2 — Move `tree/gtree.rs` free functions → `GTree` methods

Convert each free function in `src/tree/gtree.rs` to a method on `GTree`:

| Free function                                | Method                                                     |
| -------------------------------------------- | ---------------------------------------------------------- |
| `route_to_receiver(gnodes, root, x)`         | `gtree.route_to(x) -> GNodeId`                             |
| `gnode_depth_from_interval(lo, hi, n)`       | static or associated fn `GTree::depth_of_interval(lo, hi)` |
| `recompute_g_sums(gnodes, start)`            | `gtree.recompute_sums(start)`                              |
| `recompute_g_sums_subtree(gnodes, preorder)` | `gtree.recompute_sums_subtree(preorder)`                   |
| `uniform_contour_depth_of(gnodes, gid, n)`   | `gtree.uniform_contour_depth(gid)`                         |

**Steps:**

1. Add the methods to `GTree<C,V,N>`.
2. Update every call site in algorithm modules.
3. Keep the old free functions as deprecated `#[allow(dead_code)]` stubs until
   all call sites are migrated, then delete them.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move gtree free functions to GTree methods`

---

### Step 4.3 — Move split/evict G-tree logic into `GTree`

The G-tree-only parts of `split.rs` and `evict.rs` (node allocation, parent
linking, sum recomputation) can become `GTree` methods:
`gtree.allocate_children(parent_id, …)` and `gtree.merge_into_parent(id)`.

> This step requires careful reading of the algorithm modules. Only move code
> that only touches `GTree` fields (i.e., does not also mutate `VTree`).
> Leave cross-tree coordination in `GvGraph`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move G-tree mutation logic into GTree methods`

---

### Review checkpoint — Phase 4

> - Did moving the free functions into methods reveal any that need both `gnodes`
>   and `vnodes` (i.e., they are actually cross-tree and belong on `GvGraph` or
>   a future coordinator type)?
> - Is the `GTree::new()` constructor handling the `Capacity` computation cleanly,
>   or does `GvGraph::new()` still duplicate it?
> - Are `node_count`/`terminal_count` cleanly maintained through the `GTree` boundary?

---

## Phase 5 — Introduce `VTree<V>` and move the violations queue (Opportunities #2, #3)

Mirror of Phase 4.

### Step 5.1 — Define `VTree<V>` as a struct wrapper

```rust
pub(crate) struct VTree<V: Accumulator> {
    pub(crate) nodes:      Arena<VNode<V>>,
    pub(crate) root:       Option<VNodeId>,
    pub(crate) violations: Vec<VNodeId>,
}
```

**Steps:**

1. Add `VTree<V>` to the tree module.
2. In `GvGraph`, replace `vnodes`, `v_root`, and `violations` with `pub(crate) vtree: VTree<V>`.
3. Fix every access site.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): introduce VTree struct (fields only)`

---

### Step 5.2 — Move `tree/vtree.rs` free functions → `VTree` methods

| Free function                                     | Method                                               |
| ------------------------------------------------- | ---------------------------------------------------- |
| `vtree_remove_leaf(vnodes, gnodes, v_id, v_root)` | `vtree.remove_leaf(gnodes, v_id)` → returns new root |
| `propagate_v_sums(vnodes, id)`                    | `vtree.propagate_sums(id)`                           |
| `sync_intensity_in_parent(vnodes, id, val)`       | `vtree.sync_intensity(id, val)`                      |
| `recompute_all_v_intensities(vnodes)`             | `vtree.recompute_all_intensities()`                  |
| `invalidate_depth_subtree(vnodes, id)`            | `vtree.invalidate_depth(id)`                         |
| `v_depth(vnodes, id)`                             | `vtree.depth(id)`                                    |
| `propagate_evictable_flags(vnodes, id)`           | `vtree.propagate_evictable(id)`                      |
| `is_ancestor(vnodes, candidate, target)`          | `vtree.is_ancestor(candidate, target)`               |

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move vtree free functions to VTree methods`

---

### Step 5.3 — Move violations queue ownership into `VTree`

With `violations` now inside `VTree`, the violation push helpers in
`violation_push.rs` and `violation_sources.rs` become `VTree` methods:
`vtree.push_violation(id)`, `vtree.drain_violations()`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move violations queue into VTree`

---

### Step 5.4 — Move rebalance entry point into `VTree`

`rebalance::resolve(graph, id)` currently takes `&mut GvGraph`. It only writes to
`vnodes` and `violations`. After step 5.3, it can become `vtree.rebalance(id)`.

> If it also reads G-tree data (e.g., intensities from `GNode.own`), pass those as
> read-only arguments rather than moving the whole method.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(tree): move rebalance entry into VTree`

---

### Review checkpoint — Phase 5

> - After Phase 4 and Phase 5, `GvGraph` should hold: `gtree: GTree`, `vtree: VTree`,
>   `config: Config<V>`, and the feature-gated plateau state. Does it feel like a
>   clean orchestrator? Or are there fields that still don't fit?
> - Did any V-tree method need to cross to the G-tree? Document those cross-tree
>   calls explicitly — they are seams for future testing.
> - Is the violations queue lifetime now obvious? (Created per-`GvGraph`, drained
>   at the end of each `observe` call.)

---

## Phase 6 — Remove `AtomicU32 cached_depth` from `VNode` (Opportunity #7)

**Prerequisite:** Phase 5 complete (VTree exists and owns the V-arena).

### Step 6.1 — Measure the cost of uncached depth computation

Before removing the cache, add a benchmark (or a `#[test]` that calls `v_depth`
1 000 000 times) and record the time. V-tree depth is bounded by `depth_evict`
(typically ≤10), so the computation is O(depth) integer comparisons. The cache is
likely not performance-critical.

Record result here: _to fill_

### Step 6.2 — Replace `cached_depth` with a parallel `Vec<u32>` in `VTree`

```rust
pub(crate) struct VTree<V: Accumulator> {
    nodes:      Arena<VNode<V>>,
    root:       Option<VNodeId>,
    violations: Vec<VNodeId>,
    depth_cache: Vec<u32>,          // ← new: indexed by slot
    depth_dirty: Vec<bool>,         // ← or a generation counter
}
```

**Steps:**

1. Add `depth_cache` and `depth_dirty` to `VTree`.
2. Update `vtree.depth(id)` to check `depth_dirty[slot]`, compute if dirty, cache.
3. Update `vtree.invalidate_depth(id)` to set `depth_dirty` for the subtree.
4. Remove `cached_depth: AtomicU32` from `VNode`.
5. Remove the manual `Clone` impl from `VNode` (derive it now).

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(vnode): move depth cache from VNode to VTree`

### Step 6.3 — Make `VNode<V>` `Copy` (if `V: Copy`)

With `AtomicU32` gone and all fields being `Copy`-compatible when `V: Copy`, add
`#[derive(Copy)]` to `VNode`. This simplifies arena operations that currently clone.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(vnode): derive Copy when V: Copy`

---

### Review checkpoint — Phase 6

> - Did removing `AtomicU32` uncover any place that was relying on the interior
>   mutability (e.g., reading depth through a shared reference)?
> - Is the depth cache in `VTree` correctly invalidated in all split/evict code paths?
>   Run the snapshot tests with `INSTA_UPDATE=always` to check for regressions.

---

## Phase 7 — Compose the plateau subsystem (Opportunity #5)

**Prerequisite:** Phase 4 + Phase 5 complete (GTree and VTree exist).
This phase eliminates the `#[cfg(feature = "dynamic-contour-tracking")]` fields
inside `GvGraph`.

### Step 7.1 — Define a `PlateauTracking` trait

```rust
pub(crate) trait PlateauTracking<C: Coordinate, V: Accumulator> {
    fn on_observe(&mut self, gid: GNodeId, gtree: &GTree<C,V,N>);
    fn on_split  (&mut self, parent: GNodeId, left: GNodeId, right: GNodeId, gtree: &GTree<C,V,N>);
    fn on_evict  (&mut self, gid: GNodeId, gtree: &GTree<C,V,N>);
    fn plateaus  (&self) -> impl Iterator<Item = (&BasisEdge<C>, &Plateau<C,V>)>;
}
```

### Step 7.2 — Implement `DynamicPlateauTracker<C,V>` and `NoopPlateauTracker`

- `DynamicPlateauTracker` holds the current `BTreeMap<BasisEdge<C>, Plateau<C,V>>`,
  `PlateauBasis<C>`, `pending_p_i4`, and `plateaus_dirty`.
- `NoopPlateauTracker` is a zero-size type that implements all hooks as no-ops.

### Step 7.3 — Thread the tracker through `GvGraph`

Add a type parameter `P: PlateauTracking<C,V>` to `GvGraph` (or hold it as a
trait object `Box<dyn PlateauTracking<C,V>>`).

> A type parameter avoids dynamic dispatch but complicates the public API.
> A trait object is simpler for callers. Consider both and decide based on
> whether users are expected to write their own plateau trackers.

### Step 7.4 — Remove `#[cfg(feature = …)]` blocks from `GvGraph` fields

Replace the four conditional fields with a single `tracker: P` field.
Update all `#[cfg(feature = …)]` algorithm arms to call `self.tracker.on_*`.

**Validate:** `cargo test --all-features && cargo test --no-default-features`

**Commit message:** `refactor(graph): compose PlateauTracker, remove cfg fields`

---

### Review checkpoint — Phase 7

> - Does the `PlateauTracking` trait feel stable? Should it also expose
>   `plateau_for(gid)` for query algorithms?
> - Does the `PlateauRead` introduced in Phase 1 Step 1.2 compose cleanly with
>   the tracker?
> - Is there a case for making `PlateauRead` a method on `PlateauTracking` rather
>   than a separate trait?

---

## Summary and dependency order

```
Phase 1 (independent)
  1.1  PackedChildren → Children<V> enum
  1.2  Split plateaus() into PlateauRead trait
  1.3  Tighten visibility of internal spatial types

Phase 2 (independent)
  2.x  Extract StructuralConfig from Config<V>

Phase 3 (independent, easiest after Phase 1 stabilises node types)
  3.1  Encapsulate GNode fields
  3.2  Encapsulate VNode fields

Phase 4 (requires Phase 3 to reduce noise)
  4.1  GTree struct
  4.2  GTree methods from gtree.rs
  4.3  G-tree mutation logic into GTree

Phase 5 (requires Phase 4)
  5.1  VTree struct
  5.2  VTree methods from vtree.rs
  5.3  Violations queue into VTree
  5.4  Rebalance into VTree

Phase 6 (requires Phase 5)
  6.1  Benchmark depth computation
  6.2  Move depth cache to VTree
  6.3  Make VNode Copy

Phase 7 (requires Phase 4 + 5)
  7.1  PlateauTracking trait
  7.2  DynamicPlateauTracker + NoopPlateauTracker
  7.3  Thread tracker through GvGraph
  7.4  Remove #[cfg] fields from GvGraph
```

---

## Deferred / out of scope

- **Opportunity #8 (view type taxonomy) — partial.** Step 1.3 addresses
  visibility. A deeper reorganisation (introducing a common `Interval<C>` base,
  or replacing `Node<C,V>` with an opaque type) is deferred until after Phase 4,
  when the stable `GTree` API makes it clear which view types are truly public.
- **`TemporalDecay` and `WeightedSampler` trait splits** — similar to what
  Step 1.2 does for `PlateauRead`; can be done any time after Phase 1.
- **`DiscreteCoordinate` trait** — currently defined but unused in algorithmic
  traits. Review after Phase 4 to see if it should feed into `GTree<C>`.
