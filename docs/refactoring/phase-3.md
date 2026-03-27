# Phase 3 — Encapsulate node fields

**Opportunity:** [#10](../design-improvement-opportunities.md#10-gnode-and-vnode-fields-are-pubcrate--no-behavioral-boundary)

**Prerequisite:** Phase 1 complete (so `Children<V>` is stable before encapsulating `VNode`).
Independent of Phase 2.

**Goal:** Make `GNode` and `VNode` fields private within their module, replacing
direct field access with typed accessor and mutator methods. Invariant assertions
move from scattered call sites into the methods.

---

## [ ] Step 3.1 — Encapsulate `GNode<C, V>` fields

**Files touched:** `src/nodes/gnode.rs`, all algorithm modules in
`src/graph/algorithm/`, `src/tree/gtree.rs`.

**Steps:**

1. Add accessor / mutator methods to `impl GNode<C, V>`:

   | Field           | Getter              | Mutator                                                                    |
   | --------------- | ------------------- | -------------------------------------------------------------------------- |
   | `lo`, `hi`      | `lo()`, `hi()`      | _(set at construction only)_                                               |
   | `sum`           | `sum()`             | `set_sum(v: V)`                                                            |
   | `own`           | `own()`             | `set_own(v: V)`                                                            |
   | `left`, `right` | `left()`, `right()` | `link_children(l, r)`, `link_left(l)`, `link_right(r)`, `clear_children()` |
   | `parent`        | `parent()`          | `set_parent(p)`, `clear_parent()`                                          |
   | `entry`         | `entry()`           | `assign_entry(vid)`, `clear_entry()`                                       |

2. Change all fields from `pub(crate)` to `pub(super)` (private within `nodes/`).
3. Fix compilation errors in each algorithm module using the new methods.

> **Tip:** Work one algorithm file at a time. Do `cargo check` after each file,
> then commit when it compiles cleanly.

**Commit message per file:** `refactor(gnode): use accessors in <module>`
**Final commit:** `refactor(gnode): make all fields private to nodes module`

**Validate:** `cargo test --all-features`

---

## [ ] Step 3.2 — Encapsulate `VNode<V>` fields

**Files touched:** `src/nodes/vnode.rs`, all algorithm modules, `src/tree/vtree.rs`.

Same approach as Step 3.1 for `VNode`, `VKind`, and `Children<V>`.

Fields to encapsulate: `intensity`, `parent`, `cached_depth`, `kind`.

The `cached_depth: AtomicU32` field will be removed entirely in Phase 6. For now,
hide it behind a pair of methods:

- `depth_hint() -> Option<u32>` — returns cached value if valid, `None` if stale
- `invalidate_depth()` — marks the cached depth as stale

This way Phase 6 can remove the field without touching algorithm modules again.

**Commit message per file:** `refactor(vnode): use accessors in <module>`
**Final commit:** `refactor(vnode): make all fields private to nodes module`

**Validate:** `cargo test --all-features`

---

## Review checkpoint

> _Fill in after completing both steps._
>
> - Did hiding the fields reveal algorithm code that always accesses two fields
>   together (e.g., `lo + hi` always read as a pair)? If so, add a compound
>   getter: `interval() -> (C, C)`.
> - Did any mutator feel forced or awkward? That usually signals a behaviour that
>   belongs on `GNode`/`VNode` itself (e.g., `accumulate_own(delta: V)` instead of
>   `set_own(own() + delta)`).
> - After this phase, are the `debug_assert!` calls in algorithm modules now
>   redundant (moved into the mutators) or are some still needed at a higher level?
