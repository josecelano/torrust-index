# Review Index: PR #832 — Introduce Mudlark

**PR:** https://github.com/torrust/torrust-index/pull/832  
**Author:** @da2ce7  
**Branch:** `da2ce7/20260305_mudlark`  
**Reviewer:** @josecelano  
**Status:** IN PROGRESS

---

## Documents in This Review

| Document                                               | Contents                                                        |
| ------------------------------------------------------ | --------------------------------------------------------------- |
| [REVIEW_PR_832.md](REVIEW_PR_832.md)                   | Full phase-by-phase review notes (phases 1–5, author responses) |
| [findings/](findings/)                                 | Individual finding files — one per confirmed or open issue      |
| [coverage-results/](coverage-results/)                 | `cargo llvm-cov` output per run date                            |
| [mutants-results/](mutants-results/)                   | `cargo mutants` output per run date                             |
| [CODE_READING_GUIDE.md](CODE_READING_GUIDE.md)         | How to navigate the mudlark source                              |
| [MUDLARK_EXPLAINED.md](MUDLARK_EXPLAINED.md)           | High-level conceptual explanation                               |
| [GLOSSARY.md](GLOSSARY.md)                             | Term definitions                                                |
| [DESIGN_ALTERNATIVES.md](DESIGN_ALTERNATIVES.md)       | Design alternatives considered                                  |
| [USE_CASES.md](USE_CASES.md)                           | Use-case analysis                                               |
| [COORDINATE_ENGINEERING.md](COORDINATE_ENGINEERING.md) | Coordinate type engineering notes                               |
| [VERIFICATION_STRATEGY.md](VERIFICATION_STRATEGY.md)   | Test and verification strategy                                  |
| [IDEA_ANNOTATED.md](IDEA_ANNOTATED.md)                 | Annotated formal specification                                  |

---

## Review Phase Status

| Phase | Topic                     | Status     | Detail                                         |
| ----- | ------------------------- | ---------- | ---------------------------------------------- |
| 1     | Changes to existing code  | ✅ Done    | [REVIEW_PR_832.md § Phase 1](REVIEW_PR_832.md) |
| 2     | Fitness for inclusion     | ✅ Done    | [REVIEW_PR_832.md § Phase 2](REVIEW_PR_832.md) |
| 3     | Public API & `lib.rs`     | ✅ Done    | [REVIEW_PR_832.md § Phase 3](REVIEW_PR_832.md) |
| 4     | Core implementation       | ✅ Done    | [REVIEW_PR_832.md § Phase 4](REVIEW_PR_832.md) |
| 5     | Tests & benchmarks        | ✅ Done    | [REVIEW_PR_832.md § Phase 5](REVIEW_PR_832.md) |
| 6     | Documentation & ADRs      | ⬜ Pending | —                                              |
| 7     | Cargo / licensing / build | ⬜ Pending | —                                              |

---

## Findings

One file per finding in [findings/](findings/). Status key: ✅ Resolved · ⚠️ Open · ❗ Confirmed bug · ❓ Question.

| #   | Title                                                       | Status                               | File                                                                                    |
| --- | ----------------------------------------------------------- | ------------------------------------ | --------------------------------------------------------------------------------------- |
| 1   | MSRV bump 1.72 → 1.80 (workspace-wide)                      | ✅ Fixed in PR #833                  | —                                                                                       |
| 2   | `src/ui/proxy.rs` — error images silently broken            | ✅ Fixed in PR #833                  | —                                                                                       |
| 3   | `torrust-sentinel` in `cargo-machete` ignore — undocumented | ⚠️ Open (low priority)               | —                                                                                       |
| 4   | `docs/api.md` out of date (multiple missing symbols)        | ✅ Addressed by Cameron's API audit  | —                                                                                       |
| 5   | `debug_plateau_basis()` should be `#[doc(hidden)]`          | ✅ Addressed by Cameron's API audit  | —                                                                                       |
| 6   | `AGENTS.md` scope — mudlark conventions in repo-global file | ⚠️ Open (low priority)               | —                                                                                       |
| 7   | Several `pub` methods missing from `docs/api.md`            | ✅ Addressed by Cameron's API audit  | —                                                                                       |
| 8   | `GNodeInfo` not in `docs/api.md`                            | ✅ Merged into `Node` by Cameron     | —                                                                                       |
| 9   | **[BUG] f64 plateau sum drift after G-node split**          | ❗ Confirmed open                    | [finding-09-f64-plateau-drift.md](findings/finding-09-f64-plateau-drift.md)             |
| 10  | **Arena stale-handle ABA problem (no generation counters)** | ⚠️ Open — usage contract unclear     | [finding-10-arena-stale-handle.md](findings/finding-10-arena-stale-handle.md)           |
| 11  | **`range_sum` approximation not clearly documented**        | ⚠️ Open — docs gap                   | [finding-11-range-sum-approximation.md](findings/finding-11-range-sum-approximation.md) |
| 12  | **[BUG] Post-evict `debug_assert` fires in debug builds**   | ❗ Confirmed (blocks eviction tests) | [finding-12-post-evict-assert.md](findings/finding-12-post-evict-assert.md)             |

---

## Open Items (Summary)

1. **Finding #9 — f64 plateau drift** — confirmed still fires on rebased code. Needs to be reported to Cameron.
2. **Finding #10 — arena stale handle** — usage contract needs clarification or generation counters.
3. **Finding #11 — `range_sum` approximation** — not clearly documented; proptest found that `range_sum(x..x+1) = 0` even after `observe(x, 64)` until split threshold is triggered. Suggest adding `# Approximation` note to docstring.
4. **Finding #12 — post-evict `debug_assert`** — `plateau.rs:127` fires immediately after any eviction in debug builds; blocks all eviction-gated test paths. Root cause of coverage ceiling at ~87%. Needs fix (relax to `debug_assert`, soften, or remove).
5. **`torrust-sentinel` in machete ignore** — low priority, intent undocumented.
6. **`AGENTS.md` scope** — mudlark conventions belong in `packages/mudlark/AGENTS.md`.
7. **Mutation kill rate 60.1%** — borderline; re-run `cargo mutants -p torrust-mudlark` after Finding #12 is fixed; target ≥ 80% before merge.
8. **Module structure** — flat `src/` layout works but a grouped layout (nodes/tree/spatial/graph/algorithm/diagnostics) would improve navigability; propose as follow-up to Cameron.

---

## Measurement Snapshots

| Date       | Tool             | Overall                              | Detail                                                               |
| ---------- | ---------------- | ------------------------------------ | -------------------------------------------------------------------- |
| 2026-03-24 | `cargo llvm-cov` | 86.4% lines / 92.2% functions        | [llvm-cov-2026-03-24.md](coverage-results/llvm-cov-2026-03-24.md)    |
| 2026-03-25 | `cargo llvm-cov` | 86.96% lines / 96.67% fn (436 tests) | Isolated branch — see [REVIEW_PR_832.md § Phase 5](REVIEW_PR_832.md) |
| 2026-03-16 | `cargo mutants`  | 60.1% kill rate (621/1034)           | [mutants-results/](mutants-results/)                                 |
| (pending)  | `cargo mutants`  | —                                    | Re-run after Finding #12 resolved                                    |

---

## Author Response Timeline

| Date        | Topic                                            | Outcome                                 |
| ----------- | ------------------------------------------------ | --------------------------------------- |
| ~5 days ago | Non-mudlark fixes (proxy, MSRV) moved to PR #833 | PR #833 merged                          |
| ~4 days ago | Full API audit + three-test decision procedure   | API redesigned; symbols reassigned      |
| ~3 days ago | Mutation testing overhaul (ADR-M-039)            | Tests 868 → 1,311; pedagogy tests added |
