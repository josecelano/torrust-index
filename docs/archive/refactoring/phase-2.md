# Phase 2 — Decouple `Config<V>` from the value type

**Opportunity:** [#4](../design-improvement-opportunities.md#4-configv-is-parameterized-for-a-single-threshold-field)

**Prerequisite:** None — independent of all other phases.

**Files touched:** `src/graph/config.rs`, `src/graph/gv_graph.rs`,
`src/graph/traits.rs`, `tests/integration.rs`, examples.

**Goal:** A non-generic `StructuralConfig` struct so that depth / budget parameters
can be passed and inspected without knowing the value type `V`.

---

## [x] Step 2.1 — Extract `StructuralConfig`

Add a plain struct for the non-generic fields and rewrite `Config<V>` to nest it:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralConfig {
    pub depth_create:     u32,
    pub depth_evict:      u32,
    pub budget:           Option<usize>,
    pub alpha_relax:      f64,
    pub bounded_eviction: bool,
}

pub struct Config<V: Accumulator> {
    pub structural:      StructuralConfig,
    pub split_threshold: V,
}
```

**Steps:**

1. Add `StructuralConfig` to `config.rs`.
2. Rewrite `Config<V>` to contain `structural: StructuralConfig`.
3. Move or delegate `validate()` so it operates on `StructuralConfig` (the
   structural invariants) plus `split_threshold: V` (the value invariant).
4. Update every `Config { depth_create: …, … }` construction site to use the
   nested form.
5. Update every field access: `config.depth_create` → `config.structural.depth_create`.

**Validate:** `cargo test --all-features`

---

## [x] Step 2.2 — Re-export and clean up

1. Re-export `StructuralConfig` from `lib.rs`.
2. Decide whether to keep `Config<V>` as a public convenience wrapper or add a
   `From<(StructuralConfig, V)>` constructor helper.

**Validate:** `cargo test --all-features`

**Commit message:** `refactor(config): extract StructuralConfig from Config<V>`

---

## Review checkpoint

> _Filled in after completing both steps._

**Does nesting `structural: StructuralConfig` inside `Config<V>` feel ergonomic
for callers constructing configs in tests and examples?**

It is slightly more verbose but clearly communicates which fields are
structural/algorithmic vs. value-typed. The separation is most visible in the
examples where comments explaining the budget sit naturally inside
`StructuralConfig { ... }`. No `Config::new` builder was added — struct literal
construction is idiomatic Rust and callers can name the fields they mean.

**Are there places that use `Config<V>` only to read structural fields?**

`compute_capacity(&Config<V>)` in `gv_graph.rs` and `adjust_depth_gates` in
`budget.rs` — both now read only `config.structural.*`. These are internal
functions that could accept `&StructuralConfig` directly in a later clean-up
pass, but changing their signatures is outside the scope of Phase 2.
