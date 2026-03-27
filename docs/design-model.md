# Design Model

This document provides a high-level structural model of `torrust-mudlark` — types,
traits, dependencies, and relationships — without implementation details (function
bodies). It is the basis for reasoning about coupling, cohesion, and architecture
changes.

---

## 1. Public API surface

```
torrust-mudlark
├── GvGraph<C: Coordinate, V: Accumulator, const N: u32>   ← main type
├── Config<V: Accumulator>
├── GNodeId                                                 ← opaque handle
├── GState                                                  ← Terminal | SemiInternal | Internal
├── GNodeChildren                                           ← { left?, right? }
│
│   ── Read/query views ──
├── Node<C, V>
├── Span<C, V>
├── Cell<C, V>
├── Plateau<C, V>
├── BasisEdge<C>
├── BasisElement<C, V>
├── ContourRange<C, V>
├── ContourRangeEnergy<V>
│
│   ── Layered snapshot ──
├── Pewei<C, V>
│   ├── Layer<C, V>
│   │   ├── Transition<C, V>
│   │   └── Terminal<C, V>
│
│   ── Trait abstractions ──
└── traits::{
      Coordinate, Accumulator,
      Attenuatable, Weighable, Proratable, Inspectable,
      Observation, ScalableObservation,
      Rng,
      SpatialRead, SpatialWrite,
      TemporalDecay, WeightedSampler
    }
```

---

## 2. Trait hierarchy

```
Accumulator (zero, add, sub)
├── Attenuatable   – attenuate(factor: f64) → Self
├── Weighable      – weight() → f64
├── Proratable     – prorate(portion, total) → Self  /  scale_by(f64) → Self
└── Inspectable    – to_f64_approx() → f64  /  from_f64(f64) → Self

Coordinate (midpoint, width, domain_max, total_cmp, …)

Observation<V: Accumulator>   – accumulate(current: V, delta: Self) → V
└── ScalableObservation<V>    – scale(current: V, factor: Self) → V

Rng                           – next_f64() → f64

SpatialRead { Coord, Accum }  – get(coord) → Cell  /  plateaus() → Cow<BTreeMap<…>>
└── SpatialWrite              – observe(coord, delta: O: Observation)
└── TemporalDecay             – decay(root: GNodeId, attenuation, q)
└── WeightedSampler           – sample(rng) → Option<Cell>
```

`GvGraph<C,V,N>` implements **SpatialRead + SpatialWrite + TemporalDecay + WeightedSampler**
when the appropriate Accumulator sub-traits are satisfied.

| Method       | Required bounds on V                       |
| ------------ | ------------------------------------------ |
| `get`        | `Accumulator`                              |
| `observe`    | `Accumulator + Inspectable`                |
| `decay`      | `Accumulator + Attenuatable + Inspectable` |
| `sample`     | `Accumulator + Inspectable + Weighable`    |
| `plateaus()` | `Accumulator + Inspectable`                |

---

## 3. Data types and fields

### 3.1 Core nodes

```
Arena<T: Default>
  slots:    Vec<T>
  occupied: Vec<u64>    ← bitset
  free:     Vec<u32>    ← freelist of slot indices
  count:    u32

GNodeId(NonZeroU32)     ← index = value - 1
VNodeId(NonZeroU32)     ← index = value - 1

GNode<C, V>
  lo, hi:    C               ← spatial interval
  sum, own:  V               ← aggregated / own value
  left?:     GNodeId
  right?:    GNodeId
  parent?:   GNodeId
  entry?:    VNodeId         ← back-pointer to V-tree

VNode<V>
  intensity:     V
  parent?:       VNodeId
  cached_depth:  AtomicU32   ← lazily invalidated cache
  kind:          VKind<V>

VKind<V>
  = Entry     { gnode: GNodeId, is_exposed: bool, is_evictable: bool }
  | Structural { children: PackedChildren<V>,      has_evictable: bool }

PackedChildren<V>
  intensities: [V; 3]
  ids:         [Option<VNodeId>; 3]
  len:         u8             ← 2 or 3 in practice
```

### 3.2 Top-level graph

```
Config<V: Accumulator>
  split_threshold:  V
  depth_create:     u32
  depth_evict:      u32
  budget?:          usize
  alpha_relax:      f64
  bounded_eviction: bool

GvGraph<C: Coordinate, V: Accumulator, const N: u32>
  gnodes:          Arena<GNode<C, V>>
  vnodes:          Arena<VNode<V>>
  g_root:          GNodeId
  v_root?:         VNodeId
  config:          Config<V>
  violations:      Vec<VNodeId>      ← pending rebalance work
  node_count:      u32
  terminal_count:  u32
  live_depth_evict:  u32
  live_depth_create: u32
  depth_buffer:    u32
  headroom:        usize
  soft_limit?:     usize
  ── [feature: dynamic-contour-tracking] ──
  plateaus:        BTreeMap<BasisEdge<C>, Plateau<C, V>>
  pending_p_i4:    Vec<(GNodeId, BasisEdge<C>)>
  plateau_basis:   PlateauBasis<C>
  plateaus_dirty:  bool
```

### 3.3 Spatial view types (DTOs / read projections)

| Type                | Fields                                                                                         |
| ------------------- | ---------------------------------------------------------------------------------------------- |
| `Span<C,V>`         | start, end, intensity, depth                                                                   |
| `Cell<C,V>`         | start, end, intensity, depth (terminal regions only)                                           |
| `Node<C,V>`         | start, end, own, sum, depth, state, gnode_id, parent?                                          |
| `Plateau<C,V>`      | basis_edge, start, end, depth, sum                                                             |
| `BasisEdge<C>`      | newtype(C) with total ordering                                                                 |
| `BasisElement<C,V>` | gnode_id, start, end, own, sum, depth, is_boundary_thatch                                      |
| `ContourRange<C,V>` | start, end, basis[], energy, exact_energy, plateau_energy, cross_plateau_energy, plateau_count |

### 3.4 Layered snapshot (Pewei)

```
Pewei<C, V>
  domain_start, domain_end: C
  layers: Vec<Layer<C, V>>

Layer<C, V>
  transitions: Vec<Transition<C, V>>
  terminals:   Vec<Terminal<C, V>>

Transition<C, V>
  start, end: C   baseline, total, refinement: V   depth, v_depth: u32

Terminal<C, V>
  start, end: C   intensity: V   depth, v_depth: u32
```

---

## 4. Module dependency graph

```
lib.rs  (public re-exports)
│
├─── handle              [GNodeId, VNodeId]
│
├─── nodes/
│    ├── gnode           [GNode, GState, GNodeChildren]
│    └── vnode           [VNode, VKind, PackedChildren]
│
├─── arena               [Arena<T>]
│
├─── traits/
│    ├── coordinate      [Coordinate, DiscreteCoordinate]
│    ├── accumulator     [Accumulator]
│    ├── attenuatable    [Attenuatable]
│    ├── weighable       [Weighable]
│    ├── proratable      [Proratable]
│    ├── inspectable     [Inspectable]
│    ├── observation     [Observation, ScalableObservation]
│    ├── rng             [Rng]
│    ├── spatial_read    [SpatialRead]
│    ├── spatial_write   [SpatialWrite]
│    ├── temporal_decay  [TemporalDecay]
│    └── weighted_sampler[WeightedSampler]
│
├─── spatial/
│    ├── view            [Span, Cell]
│    ├── node            [Node]
│    ├── plateau         [Plateau, BasisEdge, basis_edge_of]
│    ├── plateau_basis   [PlateauBasis]          ← feature-gated
│    ├── contour_range   [ContourRange, BasisElement, ContourRangeEnergy]
│    ├── pewei           [Pewei]
│    └── pewei_types     [Layer, Transition, Terminal]
│
├─── tree/
│    ├── gtree           [route_to_receiver, recompute_g_sums, gnode_depth_from_interval]
│    └── vtree           [vtree_remove_leaf, propagate_v_sums, v_depth, invalidate_depth_subtree, …]
│
├─── graph/
│    ├── config          [Config]
│    ├── gv_graph        [GvGraph, uniform_contour_depth_of]
│    ├── traits          [SpatialRead/Write/Decay/Sampler impls for GvGraph]
│    └── algorithm/
│         ├── observe    [GvGraph::observe — 5-phase pipeline]
│         ├── query      [GvGraph::get, GvGraph::plateaus]
│         ├── split      [attempt_split, bootstrap_split]
│         ├── evict      [evict G-node, teardown V subtree]
│         ├── rebalance  [is_violated, contract, resolve]
│         ├── decay      [GvGraph::decay]
│         ├── sample     [GvGraph::sample]
│         ├── promote    [V-tree re-rooting]
│         ├── budget     [budget cap enforcement]
│         ├── violation_push / violation_sources
│         └── plateau/   [plateau sync after observe / split / evict]
│
└─── diagnostics/
     ├── invariants      (public) [check_* functions for external callers]
     ├── diagnostic      [audit_violations, diagnose_missed_violation]
     ├── display         [Display / Debug formatting helpers]
     ├── dot             [Graphviz DOT export]
     ├── dump            [text dump]
     ├── plateau_audit   [audit_plateau_consistency]  ← feature-gated
     └── plateau_invariants
```

Dependency direction:

```
traits  ←──  arena
         ←──  nodes/{gnode, vnode}
         ←──  spatial/*
         ←──  tree/{gtree, vtree}
         ←──  graph/{config, gv_graph}
         ←──  graph/algorithm/*
         ←──  diagnostics/*
```

There are no upward cycles. `graph/algorithm/*` modules are the deepest layer:
they reach into every other module.

---

## 5. Key algorithms and their phases

### `observe(coord, delta)`

1. Route coordinate → G-node (G-tree traversal)
2. Accumulate `own` on G-node
3. Propagate intensity up V-tree; enqueue violated V-nodes
4. Recompute G-tree sums up to root
5. Mirror plateau state (feature-gated)
6. Attempt split if threshold crossed
7. Process violation queue (rebalance)

### `split(g_id)`

- Guard: terminal, divisible midpoint, sum > threshold, V depth ≤ D_create
- Allocate two child G-nodes, link parent
- Insert new V-tree entry node, possibly triggering V-tree rebalance
- Invalidate depth cache; push violations

### `evict(v_id)`

- Guard: V depth ≥ D_evict, G-node is terminal, not exposed
- Remove V-tree leaf, collapse parent if single-child
- Merge G-node own value into parent G-node
- Deallocate G-node; push violations; update counts

### `decay(root, attenuation, q)`

- Collect all G-node subtrees in post-order
- Apply `Attenuatable::attenuate` to each own value
- Full V-tree recompute (post-order)
- Push violations

---

## 6. Feature flags

| Flag                                    | Effect                                                                                                                                                             |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `dynamic-contour-tracking` (default on) | Adds `plateaus`, `plateau_basis`, `pending_p_i4`, `plateaus_dirty` to `GvGraph`; enables plateau sync algorithms and `SpatialRead::plateaus()` returning live data |
| `rand` (default on)                     | Blanket `impl Rng for T: rand_core::Rng`                                                                                                                           |
| `serde` (opt-in)                        | `#[derive(Serialize, Deserialize)]` on all public data types                                                                                                       |
