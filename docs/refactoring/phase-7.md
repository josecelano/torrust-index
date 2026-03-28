# Phase 7 — `PlateauTracking` as a composable strategy

**Opportunity:** [#5](../design-improvement-opportunities.md#5-feature-flag-splits-plateau-logic-into-invisible-variant)

**Prerequisite:** Phase 4 + Phase 5 complete (both `GTree` and `VTree` exist;
`GvGraph` is now a thin orchestrator).

**Goal:** Replace `#[cfg(feature = "dynamic-contour-tracking")]` guards scattered
through `GvGraph` with a proper strategy object that is resolved at construction
time, making the feature flag a build-time switch that affects only object
construction — not every method body.

---

## [x] Step 7.1 — Define the `PlateauTracking` trait

**Files touched:** `src/traits/mod.rs` (or a new `src/traits/plateau_tracking.rs`).

```rust
pub(crate) trait PlateauTracking<C: Coordinate, V: Accumulator> {
    fn on_observe(&mut self, id: GNodeId, coord: C, value: V);
    fn on_split(&mut self, parent: GNodeId, child_lo: GNodeId, child_hi: GNodeId);
    fn on_evict(&mut self, id: GNodeId);

    /// Read-only access to the currently tracked plateaus.
    fn plateaus(&self) -> &[Plateau<C, V>];
}
```

**Steps:**

1. Add the trait to the traits module, re-export from `src/traits/mod.rs`.
2. Run `cargo check` — the trait is unused but should compile clean.
3. Decide whether to keep the trait `pub(crate)` or `pub` (for external implementors).

**Validate:** `cargo check --all-features`

**Commit message:** `refactor(traits): define PlateauTracking strategy trait`

---

## [x] Step 7.2 — Implement `DynamicPlateauTracker<C,V>` and `NoopPlateauTracker`

**Files touched:** `src/graph/` or a new `src/plateau/` module (your call).

```rust
/// The real tracker — gated behind the feature.
#[cfg(feature = "dynamic-contour-tracking")]
pub(crate) struct DynamicPlateauTracker<C, V> { … }

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V>
    for DynamicPlateauTracker<C, V>
{
    fn on_observe(&mut self, …) { /* existing logic extracted here */ }
    fn on_split(&mut self, …)   { /* existing logic */ }
    fn on_evict(&mut self, …)   { /* existing logic */ }
    fn plateaus(&self) -> &[Plateau<C, V>] { &self.plateaus }
}

/// Zero-cost no-op for builds without the feature.
pub(crate) struct NoopPlateauTracker;

impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V> for NoopPlateauTracker {
    fn on_observe(&mut self, ..) {}
    fn on_split(&mut self, ..)   {}
    fn on_evict(&mut self, ..)   {}
    fn plateaus(&self) -> &[Plateau<C, V>] { &[] }
}
```

**Steps:**

1. Extract the existing plateau logic from `GvGraph` into `DynamicPlateauTracker`.
2. Write `NoopPlateauTracker`.
3. Both must implement the new trait without changing any public interface.

**Validate:** `cargo test --all-features && cargo test` (both feature sets)

**Commit message:** `refactor(plateau): extract DynamicPlateauTracker and NoopPlateauTracker`

---

## [x] Step 7.3 — Thread the tracker through `GvGraph`

**Design decision — trait object vs type parameter:**

| Approach                                           | Pros                        | Cons                                           |
| -------------------------------------------------- | --------------------------- | ---------------------------------------------- |
| `tracker: Box<dyn PlateauTracking<C,V>>`           | Simple; no extra type param | Vtable overhead per call; `Box` allocation     |
| `GvGraph<C,V,N,T: PlateauTracking<C,V>>`           | Zero-cost; inlined          | Adds a type param; harder to change at runtime |
| Opaque type via `impl Trait` in fields (Rust 2024) | Clean syntax                | Still a type param at monomorphisation         |

**Recommendation:** Use the type-param approach and provide type aliases:

```rust
#[cfg(feature = "dynamic-contour-tracking")]
pub type DefaultGraph<C,V,N> = GvGraph<C,V,N,DynamicPlateauTracker<C,V>>;

#[cfg(not(feature = "dynamic-contour-tracking"))]
pub type DefaultGraph<C,V,N> = GvGraph<C,V,N,NoopPlateauTracker>;
```

**Steps:**

1. Add the type parameter `T: PlateauTracking<C,V>` to `GvGraph`.
2. Replace `#[cfg(feature = "dynamic-contour-tracking")]` plateau fields with
   `tracker: T`.
3. Replace each guarded block:

   ```rust
   // before
   #[cfg(feature = "dynamic-contour-tracking")]
   self.plateau_tracker.do_thing();

   // after
   self.tracker.on_observe(id, coord, value);
   ```

4. Add the type aliases above.
5. Update `Config` if it currently carries plateau-related config; move that into
   the tracker's constructor.

**Validate:** `cargo test --all-features && cargo test`

**Commit message:** `refactor(graph): replace cfg-guarded plateau fields with PlateauTracking type param`

---

## [ ] Step 7.4 — Remove all remaining `#[cfg(feature = "dynamic-contour-tracking")]` guards from `GvGraph`

**Files touched:** `src/graph/gv_graph.rs`, algorithm modules.

After Steps 7.1–7.3 the only place the feature flag should appear is:

- `Cargo.toml` (feature definition)
- The concrete tracker implementations (`dynamic_plateau_tracker.rs`)
- The module gating those impls
- The `DefaultGraph` type alias

Scan for any remaining `#[cfg(feature = "dynamic-contour-tracking")]` in
`gv_graph.rs` and algorithm files; each can be replaced with a trait-method call.

```bash
grep -rn 'cfg(feature = "dynamic-contour-tracking")' src/
```

Target: the above command should return only the tracker module and type-alias
definition.

**Validate:** `cargo test --all-features && cargo test`

**Commit message:** `refactor(graph): remove cfg guards from gv_graph algorithm code`

---

## Review checkpoint

> _Fill in after completing all steps._
>
> - How many lines of `cfg`-guarded code were removed from `GvGraph`?
> - Does the `NoopPlateauTracker` path compile to zero overhead (confirm with
>   `cargo asm` or `cargo bloat` if curious)?
> - Are there any other feature-flag guards in the codebase that could benefit
>   from the same pattern? (`serde`, `rand`?)
> - After Phase 4–7, `GvGraph` should be: `{ config, gtree, vtree, tracker }`.
>   Does it fit on one screen? Is its `observe()` body readable as a narrative?
