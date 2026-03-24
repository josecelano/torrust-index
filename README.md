# torrust-mudlark

> **φ-Bounded Geometric-Value Graph** — an adaptive streaming spatial density estimator.

A [mudlark](https://en.wikipedia.org/wiki/Mudlarking) scavenges the riverbank for things of value. This library does the same: it finds where value is concentrated in a noisy stream of observations over a 1D coordinate space.

---

## What it does

`torrust-mudlark` answers: _"where is activity concentrated right now?"_

You feed it a stream of `(coordinate, value)` observations. It maintains an adaptive histogram over a fixed 1D domain, automatically splitting high-activity sub-ranges at finer resolution (guided by the golden ratio φ) while keeping quiet regions coarse. Decay shrinks stale observations over time, so the structure tracks a **sliding window of activity**.

The three primary operations:

| Operation                     | Description                                                        |
| ----------------------------- | ------------------------------------------------------------------ |
| `observe(coord, delta)`       | Record activity of magnitude `delta` at `coord`                    |
| `decay(root, attenuation, q)` | Age out old observations (temporal forgetting)                     |
| `sample(rng)`                 | Draw a coordinate, weighted proportionally to accumulated activity |

A fourth operation, `extract()`, serialises the current distribution into a `Pewei` — a compact, layered snapshot that can be sent over the network or persisted, and later reconstructed via `Pewei::reconstruct()`.

---

## Core type

```rust
let graph: GvGraph<u32, u64, 16> = GvGraph::new(Config { ... });
//                 ^     ^    ^
//                 │     │    └── address-space depth (domain = [0, 2^N - 1])
//                 │     └─────── accumulated value type (u64 counts, bytes, …)
//                 └───────────── coordinate type (u32 key, port, prefix, …)
```

Internally the graph is a dual tree:

- **G-tree** — the geometric skeleton; partitions the coordinate space recursively.
- **V-tree** — the value tree; accumulates observations and drives rebalancing.

Splits occur when a node accumulates enough value relative to its siblings, and evictions reclaim nodes when the budget is exhausted.

---

## Use cases

The structure is domain-agnostic. Likely applications:

- **BitTorrent tracker** (the Torrust context): coordinate = peer-ID or info-hash prefix; observe = peer announce; decay = peer departure; sample = route to a hot swarm region. `Pewei` snapshots can be gossiped between tracker nodes.
- **Adaptive frequency estimation**: find heavy hitters in a high-cardinality key stream.
- **Load balancing**: track per-shard-key request rates; route to hot shards.
- **Network traffic analysis**: monitor per-port or per-prefix activity over time.
- **Probabilistic caching**: sample eviction candidates weighted by recency × frequency.

---

## Feature flags

| Feature                    | Default | Description                                                   |
| -------------------------- | ------- | ------------------------------------------------------------- |
| `dynamic-contour-tracking` | ✓       | Maintain contour ranges for efficient range queries           |
| `rand`                     | ✓       | Implement `WeightedSampler` via `rand_core::RngCore`          |
| `serde`                    | ✗       | `Serialize`/`Deserialize` for `GvGraph`, `Pewei`, and friends |

---

## Documentation

- [docs/architecture.md](docs/architecture.md) — component and call-flow diagrams
- [docs/refactor-module-structure.md](docs/refactor-module-structure.md) — proposed module restructure
- [docs/test-plan-pre-refactor.md](docs/test-plan-pre-refactor.md) — test safety net plan
- [docs/coverage-baseline.md](docs/coverage-baseline.md) — test coverage baseline (~60% lines)

---

## Status

Early stage — code is complete and all invariants pass, but documentation, examples, and benchmarks are still being added. See [docs/refactor-module-structure.md](docs/refactor-module-structure.md) for planned improvements.

---

## License

AGPL-3.0-only — see [LICENSE](LICENSE) for details.
