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
| [REVIEW_PR_832.md](REVIEW_PR_832.md)                   | Full phase-by-phase review notes (phases 1–4, author responses) |
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
| 5     | Tests & benchmarks        | ⬜ Pending | —                                              |
| 6     | Documentation & ADRs      | ⬜ Pending | —                                              |
| 7     | Cargo / licensing / build | ⬜ Pending | —                                              |

---

## Findings

One file per finding in [findings/](findings/). Status key: ✅ Resolved · ⚠️ Open · ❗ Confirmed bug · ❓ Question.

| #   | Title                                                       | Status                              | File                                                                          |
| --- | ----------------------------------------------------------- | ----------------------------------- | ----------------------------------------------------------------------------- |
| 1   | MSRV bump 1.72 → 1.80 (workspace-wide)                      | ✅ Fixed in PR #833                 | —                                                                             |
| 2   | `src/ui/proxy.rs` — error images silently broken            | ✅ Fixed in PR #833                 | —                                                                             |
| 3   | `torrust-sentinel` in `cargo-machete` ignore — undocumented | ⚠️ Open (low priority)              | —                                                                             |
| 4   | `docs/api.md` out of date (multiple missing symbols)        | ✅ Addressed by Cameron's API audit | —                                                                             |
| 5   | `debug_plateau_basis()` should be `#[doc(hidden)]`          | ✅ Addressed by Cameron's API audit | —                                                                             |
| 6   | `AGENTS.md` scope — mudlark conventions in repo-global file | ⚠️ Open (low priority)              | —                                                                             |
| 7   | Several `pub` methods missing from `docs/api.md`            | ✅ Addressed by Cameron's API audit | —                                                                             |
| 8   | `GNodeInfo` not in `docs/api.md`                            | ✅ Merged into `Node` by Cameron    | —                                                                             |
| 9   | **[BUG] f64 plateau sum drift after G-node split**          | ❗ Confirmed open                   | [finding-09-f64-plateau-drift.md](findings/finding-09-f64-plateau-drift.md)   |
| 10  | **Arena stale-handle ABA problem (no generation counters)** | ⚠️ Open — usage contract unclear    | [finding-10-arena-stale-handle.md](findings/finding-10-arena-stale-handle.md) |

---

## Open Items (Summary)

1. **Finding #9 — f64 plateau drift** — confirmed still fires on rebased code. Needs to be reported to Cameron.
2. **Finding #10 — arena stale handle** — usage contract needs clarification or generation counters.
3. **`torrust-sentinel` in machete ignore** — low priority, intent undocumented.
4. **`AGENTS.md` scope** — mudlark conventions belong in `packages/mudlark/AGENTS.md`.
5. **New mutation kill rate** — re-run `cargo mutants -p torrust-mudlark` and record in [mutants-results/](mutants-results/).

---

## Measurement Snapshots

| Date       | Tool             | Overall                       | Detail                                                            |
| ---------- | ---------------- | ----------------------------- | ----------------------------------------------------------------- |
| 2026-03-24 | `cargo llvm-cov` | 86.4% lines / 92.2% functions | [llvm-cov-2026-03-24.md](coverage-results/llvm-cov-2026-03-24.md) |
| (original) | `cargo mutants`  | 60.1% kill rate               | [mutants-results/](mutants-results/)                              |
| (pending)  | `cargo mutants`  | —                             | Re-run after Cameron's +443 tests                                 |

---

## Author Response Timeline

| Date        | Topic                                            | Outcome                                 |
| ----------- | ------------------------------------------------ | --------------------------------------- |
| ~5 days ago | Non-mudlark fixes (proxy, MSRV) moved to PR #833 | PR #833 merged                          |
| ~4 days ago | Full API audit + three-test decision procedure   | API redesigned; symbols reassigned      |
| ~3 days ago | Mutation testing overhaul (ADR-M-039)            | Tests 868 → 1,311; pedagogy tests added |
