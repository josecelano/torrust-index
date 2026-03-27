# Phase 3 — Encapsulate node fields

**Opportunity:** [#10](../design-improvement-opportunities.md#10-gnode-and-vnode-fields-are-pubcrate--no-behavioral-boundary)

**Prerequisite:** Phase 1 complete (so `Children<V>` is stable before encapsulating `VNode`).
Independent of Phase 2.

**Goal:** Make `GNode` and `VNode` fields private within their module, replacing
direct field access with typed accessor and mutator methods. Invariant assertions
move from scattered call sites into the methods.

---

## [x] Step 3.1 — Encapsulate `GNode<C, V>` fields

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

## [x] Step 3.2 — Encapsulate `VNode<V>` fields

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

_Completed in commit `d9cec0c` — 27 files changed._

- **Compound getters:** `lo` and `hi` are read together in several algorithm modules (e.g.,
  `split.rs`, `evict.rs`). No compound getter was added yet; the pair is still read individually.
  If Phase 4 moves G-tree operations into `GTree`, adding `interval() -> (C, C)` at that point
  would be the natural place.
- **Forced mutators:** `set_own` in `evict.rs` felt slightly forced — the caller pre-reads `own()`
  into a local variable before the mutable borrow to avoid `E0502`, then passes the combined value
  to `set_own`. An `accumulate_own(delta: V)` method would remove that pattern.
- **Dead code removed:** `link_children`, `clear_parent` (GNode), `depth_hint`, `clear_parent`,
  `invalidate_depth` (VNode), and `set_parent` (GNode) were never called outside the module and
  were removed rather than suppressed.
- **`cached_depth` accessor design:** `depth_hint() -> Option<u32>` was added but turned out unused;
  callers only ever need the raw value via `cached_depth_raw()`. Removed. Phase 6 will hide
  `AtomicU32` entirely.
- **`or_fun_call` lint:** Several callers used `.or(g.right())` on Option — now `.or_else(|| g.right())`
  after all accessors became `const fn`. The clippy lint fired because `const fn` was not initially
  applied; adding `const` to all trivial accessors silenced it.
- **Test false positives:** The bulk-conversion script incorrectly added `()` to public-field structs
  (`GNodeChildren.left/.right`, `Cell.intensity`, `Span.intensity`). These were reverted to plain
  field access since those structs are not encapsulated in this phase.
