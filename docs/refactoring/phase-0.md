# Phase 0 — Baseline

**Goal:** Record the green baseline and ensure tooling is clean before any changes.

---

## [ ] Step 0.1 — Record baseline test counts

```bash
cargo test --all-features 2>&1 | grep "test result"
```

| Suite       | Passed    |
| ----------- | --------- |
| lib (unit)  | _to fill_ |
| integration | _to fill_ |
| snapshot    | _to fill_ |

---

## [ ] Step 0.2 — Confirm clippy is clean

```bash
cargo clippy --all-features -- -D warnings
```

Result: _to fill_

---

## Review checkpoint

> _Fill in after completing phase 0._
