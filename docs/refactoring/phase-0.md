# Phase 0 — Baseline

**Goal:** Record the green baseline and ensure tooling is clean before any changes.

---

## [x] Step 0.1 — Record baseline test counts

```bash
cargo test --all-features 2>&1 | grep "test result"
```

| Suite       | Passed |
| ----------- | ------ |
| lib (unit)  | 404    |
| integration | 10     |
| snapshot    | 4      |

---

## [x] Step 0.2 — Confirm clippy is clean

```bash
cargo clippy --all-features -- -D warnings
```

Result: ✅ 0 errors (57 pre-existing errors fixed in commit `8176385`).

Categories resolved:

- `redundant_pub_crate` (40): dropped `pub(crate)` inside already-`pub(crate)` parents
- `missing_panics_doc` (4): added `# Panics` sections to `decay`, `get`, `range_sum`, `dump_gtree_dot`
- `doc_list_item_overindented` (3): fixed malformed doc list in `plateau/normalise.rs`
- `doc_markdown` (2): added backticks around `SemiInternal` in `diagnostics/dot.rs`
- `float_cmp` (2): replaced `!=` + abs check with abs-only check in `invariants.rs`
- `wildcard_import` (1): expanded `use super::violation_push::*` in `rebalance.rs`
- `collapsible_if` (1): collapsed nested `if` in `rebalance.rs`
- `const_fn_trait_bound` (1): `fn` → `const fn` for `depth_plus_one` in `split.rs`
- `too_many_lines` (3): `#[allow]` on `evict_tip`, `legacy_promote`, `place_basis_element`

---

## Review checkpoint

Phase 0 complete. Commit `8176385` on branch `review/pr-832-mudlark-isolated`.

- 418 tests passing (404 unit + 10 integration + 4 snapshot)
- `cargo clippy --all-features` exits clean
- No behaviour changes — all modifications are visibility qualifiers, doc annotations, and lint suppressions
