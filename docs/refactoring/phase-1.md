# Phase 1 — Low-risk, self-contained improvements

These three steps touch one file or one concept each. They are independent and can
be done in any order.

**Prerequisite:** Phase 0 complete (baseline recorded, clippy clean).

---

## [ ] Step 1.1 — `PackedChildren<V>` → `Children<V>` enum

**Opportunity:** [#6](../design-improvement-opportunities.md#6-packedchildrenv-encodes-an-invariant-as-a-runtime-field)

**Files touched:** `src/nodes/vnode.rs`, callers in `src/graph/algorithm/`.

**What:** Replace the `PackedChildren<V> { intensities, ids, len }` struct with a
typed enum that makes the 2-or-3 children invariant unrepresentable to violate:

```rust
pub(crate) enum Children<V> {
    Pair  { ids: [VNodeId; 2], intensities: [V; 2] },
    Triple{ ids: [VNodeId; 3], intensities: [V; 3] },
}
```

**Design note — why an enum and not a `Vec` / `SmallVec`:**

The V-tree is a **2-3 tree**. The maximum child count per structural node is not
an arbitrary capacity choice; it is a hard mathematical invariant of the algorithm:

- `promote()` transitions a 2-node → 3-node (asserts `len == 2` before)
- `contract()` transitions a 3-node → 2-node (asserts `len == 3` before)
- No other arities exist anywhere in the algorithm layer

A `Vec<_>` or `SmallVec<_>` would remain flexible at the type level but the
algorithm would still assert exact counts at runtime — hiding the constraint
rather than documenting it. The enum _is_ the invariant. If a future algorithm
change required a fourth child (e.g., widening to a B-tree), the right action is
to add a `Quad` variant consciously, making the algorithm change visible in the
type system. That is a feature, not a limitation.

**Steps:**

1. Add `Children<V>` alongside `PackedChildren<V>` in `vnode.rs`.
2. Add `From<PackedChildren<V>> for Children<V>` to migrate callers gradually.
3. Change `VKind::Structural { children }` field type from `PackedChildren<V>` to `Children<V>`.
4. Update all match arms in algorithm modules (`rebalance`, `split`, `evict`, `vtree`).
5. Remove `PackedChildren<V>` and the `From` conversion.
6. Remove all `assert!(index < self.len())` guards that are now unreachable.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(nodes): replace PackedChildren with Children enum`

---

## [ ] Step 1.2 — Split `plateaus()` into a separate capability trait

**Opportunity:** [#9](../design-improvement-opportunities.md#9-spatialreadplateaus-leaks-internal-collection-type-through-the-trait)

**Files touched:** `src/traits/spatial_read.rs`, new `src/traits/plateau_read.rs`,
`src/graph/traits.rs`, `src/diagnostics/`, examples.

**What:** Move `plateaus()` out of `SpatialRead` into a new `PlateauRead` trait so
that the `Cow<BTreeMap<…>>` return type and the `BasisEdge<C>` key stay out of the
general read interface:

```rust
pub trait PlateauRead: SpatialRead {
    fn plateaus(
        &self,
    ) -> impl Iterator<Item = (&BasisEdge<Self::Coord>, &Plateau<Self::Coord, Self::Accum>)>;
}
```

**Steps:**

1. Create `src/traits/plateau_read.rs` with the `PlateauRead` trait.
2. Re-export from `src/traits/mod.rs`.
3. Implement `PlateauRead` for `GvGraph` in `src/graph/traits.rs` (use the existing
   `Cow<BTreeMap<…>>` as an internal adapter).
4. Update all call sites (diagnostics, examples, tests) to use `PlateauRead`.
5. Remove `plateaus()` from `SpatialRead`.
6. Remove the `Cow` import from `spatial_read.rs`.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(traits): extract PlateauRead from SpatialRead`

---

## [ ] Step 1.3 — Tighten visibility of internal spatial types

**Opportunity:** [#8](../design-improvement-opportunities.md#8-overlapping-spatial-view-types-create-an-unclear-taxonomy)

**Files touched:** `src/spatial/contour_range.rs`, `src/lib.rs`.

**What:** `BasisElement<C,V>` and `ContourRange<C,V>` are scratch types used only
inside the algorithm layer. Callers outside the crate should not depend on them.

**Steps:**

1. Change `pub struct BasisElement` and `pub struct ContourRange` to `pub(crate)`.
2. Check `lib.rs` — remove any re-exports of these types.
3. `cargo check --all-features` to verify no external callers break.
4. Annotate `Node<C,V>` in `src/spatial/node.rs` with a TODO:
   ```rust
   // TODO(opportunity-8): Node exposes gnode_id (an internal handle);
   // consider replacing with an opaque query result in a later phase.
   ```
   This is deferred — changing `Node`'s public shape is lower priority.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(spatial): tighten visibility of internal contour types`

---

## Review checkpoint

> _Fill in after completing all three steps._
>
> - Did the `Children<V>` enum reveal any match arms that were previously hidden
>   by the `len` field (e.g., an implicit `len == 1` or `len == 0` path)?
> - Does `PlateauRead` as a separate trait feel right in practice? Should
>   `WeightedSampler` be split out of `SpatialRead` the same way?
> - Are there other `pub` types that are only used internally and could be
>   moved to `pub(crate)`?
