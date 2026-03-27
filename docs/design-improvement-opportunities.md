# Design Improvement Opportunities

This document records high-level design issues found from structural analysis of the
codebase (types, traits, dependencies, module boundaries) — **not** from reading
function bodies. Each entry names the issue, the pattern it violates, and a concrete
direction for improvement.

See [design-model.md](design-model.md) for the structural model this analysis is
based on.

---

## 1. `GvGraph` is a God Object

**What:** `GvGraph<C,V,N>` owns ≥14 fields across two tree arenas, configuration,
operational scheduling state, and a feature-flag-conditional subsystem. Every
algorithm module attaches to it via `impl` blocks.

**Pattern violated:** Single Responsibility Principle. A type that changes for
many independent reasons (tree structural changes, scheduling policy, plateau
tracking, budget management) accumulates risk and friction.

**Direction:**

- Extract a `GTree<C,V,N>` (G-arena + g_root + counts + tree query)
- Extract a `VTree<V>` (V-arena + v_root + violations queue)
- Extract a `PlateauTracker<C,V>` (the feature-gated fields as a composed subsystem)
- Reduce `GvGraph` to a thin orchestrator that delegates to these sub-objects

---

## 2. The two trees have no explicit types

**What:** The dual G-tree / V-tree relationship is the core concept of the crate, but
there is no `GTree` or `VTree` type. Tree operations in `tree/gtree.rs` and
`tree/vtree.rs` are free functions taking raw `&mut Arena<GNode<C,V>>` arguments.
The root node and element counts are managed by the enclosing `GvGraph`.

**Pattern violated:** Missing domain objects. The domain concepts that most need to
carry and enforce invariants have no home.

**Direction:**

- `GTree` owns the arena + root + counts and exposes methods: `route_to(coord)`,
  `recompute_sums(id)`, `depth_of(id)`, `split(id)`, `evict(id)`.
- `VTree` owns its arena + root + violations queue and exposes: `insert_entry(gid)`,
  `remove_leaf(vid)`, `propagate(vid)`, `is_violated(vid)`, `rebalance()`.
- Free functions in `tree/` become methods; the `GvGraph` calls them by name.

---

## 3. Violations queue is scheduling state mixed into structural data

**What:** `violations: Vec<VNodeId>` lives inside `GvGraph`. This list is
operational scheduling state (nodes awaiting rebalancing) with a different lifecycle
and purpose than tree structure or configuration. The `observe` pipeline both
produces and drains violations in the same call.

**Pattern violated:** Separation of concerns / Single Responsibility.

**Direction:**

- If `VTree` is introduced (see #2), the violations queue belongs there.
- Alternatively, pass a `PendingWork` struct through mutation pipelines so the
  scheduling concern is explicit and local.

---

## 4. `Config<V>` is parameterized for a single threshold field

**What:**

```rust
pub struct Config<V: Accumulator> {
    pub split_threshold: V,   // ← sole reason for the V type parameter
    pub depth_create: u32,
    pub depth_evict: u32,
    pub budget: Option<usize>,
    pub alpha_relax: f64,
    pub bounded_eviction: bool,
}
```

The entire `Config` type carries `V` just to hold one threshold. This propagates
the value type into every site that stores or passes a `Config`.

**Pattern violated:** Unnecessary coupling via generics.

**Direction:**

- Separate structural configuration (`depth_create`, `depth_evict`, `budget`,
  `alpha_relax`, `bounded_eviction`) from the value-typed threshold.
- Structural config can be a plain non-generic `struct StructuralConfig`.
- The threshold can be held as a separate `split_threshold: V` field directly on
  `GvGraph`, or wrapped in a small `Threshold<V>` newtype.
- This would let `StructuralConfig` be used in contexts that don't know `V`.

---

## 5. Feature-flag conditional fields fragment the core struct

**What:** Four fields in `GvGraph` are `#[cfg(feature = "dynamic-contour-tracking")]`.
The feature flag makes the struct layout non-uniform and the algorithm modules
conditionally compile around it.

**Pattern violated:** Open/Closed Principle and composability.

**Direction:**

- Define a `PlateauTracking` trait with `on_observe`, `on_split`, `on_evict` hooks.
- Provide a `DynamicPlateauTracker<C,V>` (enabled) and a `NoopPlateauTracker` (disabled).
- `GvGraph` holds `tracker: T: PlateauTracking` and delegates to it unconditionally.
- The feature flag selects which type is used by default, but the struct shape is stable.

---

## 6. `PackedChildren<V>` encodes an invariant as a runtime field

**What:** `PackedChildren` uses a fixed-size `[…; 3]` array plus a `len: u8` field.
The invariant "a structural V-node always has 2 or 3 children" is enforced only by
`debug_assert!` at access time.

**Pattern violated:** Making illegal states unrepresentable (type-level invariants).

**Direction:**

```rust
enum Children<V> {
    Pair  { ids: [VNodeId; 2], intensities: [V; 2] },
    Triple{ ids: [VNodeId; 3], intensities: [V; 3] },
}
```

- Eliminates the `assert!(index < self.len())` guards.
- Makes `contract` (3 → 2) and `expand` (2 → 3) operations return concrete types.
- Communicates the branching factor constraint to readers without needing comments.

---

## 7. `AtomicU32 cached_depth` is a caching concern inside a data node

**What:** `VNode.cached_depth: AtomicU32` is a derived, lazily-recomputed value
stored inside the node. This requires a manual `Clone` impl (the auto-derived one
is suppressed by `AtomicU32`), introduces interior mutability into an otherwise
value-semantic struct, and couples the caching strategy to the data layout.

**Pattern violated:** Single Responsibility — a data type should not own caching
strategy.

**Direction:**

- Move the depth cache outside `VNode`: a parallel `Vec<u32>` in `VTree` indexed by
  slot, with a generation counter to mark stale entries.
- Or compute depth on demand without caching (measure first: V-tree depth is bounded
  and the computation is cheap).
- `VNode` becomes a plain `Copy` struct, simplifying all arena operations involving it.

---

## 8. Overlapping spatial view types create an unclear taxonomy

**What:** `Node<C,V>`, `Span<C,V>`, `Cell<C,V>`, `BasisElement<C,V>`,
`Transition<C,V>`, `Terminal<C,V>` all carry `(start, end, depth)` with varying
extra fields. The distinctions are inferable only from doc-comments and usage context.

**Pattern violated:** Expressiveness / clear domain language.

**Direction:**

- Establish a named base (even if not a Rust base type): an `Interval<C>` value
  object `{ start, end }` that all region types compose.
- Distinguish clearly between **query results** visible to callers (`Cell`, `Span`,
  the `Pewei` types) and **internal scratch types** (`BasisElement`, `ContourRange`),
  moving the latter to `pub(crate)`.
- Consider whether `Node<C,V>` (which exposes `gnode_id`) should remain public at all,
  or be replaced by a more opaque query result.

---

## 9. `SpatialRead::plateaus()` leaks internal collection type through the trait

**What:**

```rust
fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>>;
```

The return type exposes `Cow`, `BTreeMap`, and the internal `BasisEdge<C>` key in the
public trait signature. The `Cow` arises as an implementation detail of the
feature-gated backing store.

**Pattern violated:** Encapsulation — implementation details in abstract interfaces.

**Direction:**

- Return an opaque iterator (`impl Iterator<Item = (&BasisEdge<C>, &Plateau<C,V>)>`)
  or a thin view wrapper (`PlateauView<'_>`).
- The `Cow` optimization stays as an implementation detail inside `GvGraph`.
- Consider whether `plateaus()` belongs in `SpatialRead` or in a separate
  `PlateauRead` capability trait that only callers needing plateaus depend on.

---

## 10. `GNode` and `VNode` fields are `pub(crate)` — no behavioral boundary

**What:** All fields on `GNode` and `VNode` are `pub(crate)`, so every algorithm
module reads and writes any field freely. Invariants (e.g., "if `entry` is Some,
the pointed-to VNode must have `gnode` pointing back") are enforced only by
`debug_assert!` calls scattered across algorithm files.

**Pattern violated:** Encapsulation / Design by Contract.

**Direction:**

- Make fields private to the `nodes/` module.
- Expose typed mutation methods:
  - `GNode::link_children(left: GNodeId, right: GNodeId)`
  - `GNode::assign_entry(vid: VNodeId)`
  - `VNode::link_to_gnode(gid: GNodeId)`
- Invariant assertions move from scattered call sites into these methods, making
  violations detectable at the one place where the state changes.

---

## Priority ranking

| #   | Issue                                  | Effort         | Impact                         |
| --- | -------------------------------------- | -------------- | ------------------------------ |
| 1+2 | God Object / missing GTree+VTree types | High           | High — unlocks all others      |
| 6   | `PackedChildren` type-level invariant  | Low            | Medium — immediate safety gain |
| 10  | Node field encapsulation               | Medium         | Medium — invariant safety      |
| 3   | Violations queue placement             | Low (after #2) | Medium                         |
| 4   | `Config<V>` generics reduction         | Low            | Low-medium — API clarity       |
| 7   | `cached_depth` moved out of VNode      | Medium         | Medium — simplifies Clone      |
| 5   | Feature-flag composition               | Medium         | Low-medium — extensibility     |
| 8   | View type taxonomy                     | Low            | Low — expressiveness           |
| 9   | `plateaus()` trait signature           | Low            | Low — API hygiene              |

The highest-leverage starting point is **#1 + #2**: introducing proper `GTree` and
`VTree` types. This creates natural homes for the violations queue (#3), the depth
cache (#7), and the plateau tracker (#5), and it makes the node field encapsulation
(#10) enforceable within a bounded module boundary.
