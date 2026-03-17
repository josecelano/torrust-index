# `torrust-mudlark` Glossary

Precise definitions for terms that appear in the mudlark source code, ADRs, and
documentation. Where a term's name may surprise readers, an explanation of the
naming rationale is included.

---

## coordinate

**Definition:** A position in the index's `[0, 2^N)` key space — the _where_ of
an observation. It is the lookup key, not a measured quantity and not a
configuration parameter.

**Why this name:** In a `HashMap` or B-Tree a key is an opaque identifier — its
numeric value has no effect on structure. In mudlark the numeric value _drives_
the structure: the tree routes left or right by comparing against midpoints, and
splits halve intervals at their exact numeric midpoint. This makes the input a
_geometrically positioned point_, which is what a coordinate is in mathematics: a
value that locates a point on an axis. The name places mudlark explicitly in the
1D segment-tree and spatial-index literature, where "coordinate" is standard.

**Note on dimensionality:** Most programmers encounter coordinates as pairs or
tuples: (x, y), (lat, lon), (row, col). mudlark uses a _single_ numeric axis
only — one coordinate per observation. A coordinate system with one axis is
perfectly valid in mathematics but may surprise readers without a spatial-index
background. Think of it as the position on a number line, not a point in a plane.

**Examples:**

- Infohash prefix `0xAB` = 171 in `[0, 256)` — where on the infohash axis this announce lands.
- Listening port `6881` — position on the port-number axis.
- IP /8 prefix `10` — position on the IP-space axis.

---

## intensity

**Definition:** The weight carried by a single call to `observe(coord, intensity)`
— the _how much_ of an observation. Always a non-negative integer.

**Why this name:** "Count" would imply each call contributes exactly 1 unit.
"Weight" is accurate but generic. "Intensity" captures that the value measures
the _strength_ or _magnitude_ of activity at a coordinate at a point in time,
which fits naturally with the decay and budget semantics (strong signals resist
decay and eviction; weak signals do not).

**Examples:**

- `1` for a single `event=started` announce.
- `5` if you batch five concurrent announces into one `observe` call.
- A higher value could represent a weighted signal (e.g. a completed download
  weighted heavier than a re-announce).

---

## `own`

**Definition:** The intensity accumulated _directly_ at a G-node, via
observations routed to it before it was split. Frozen permanently once the node
gains its first child.

**Why this name:** Contrasts with `sum`. A node "owns" only what arrived at it
directly — it does not own what its children later accumulate. Once children
exist, all future observations bypass the parent, so `own` stops growing and
becomes a permanent historical record.

**Examples:**

- A node covering `[64, 128)` that received 250 announces before its first split
  has `own = 250` forever, even as its children accumulate thousands more.

---

## `sum`

**Definition:** A G-node's total subtree intensity: `own` plus the `sum` of every
descendant. Keeps growing even after `own` is frozen.

**Why this name:** It is literally the arithmetic sum of all intensity ever
observed anywhere under this node's interval.

**Examples:**

- Same node as above, after its children accumulate 1 000 more announces:
  `own = 250`, `sum = 1 250`.

---

## G-node

**Definition:** A node in the G-Tree. Covers a half-open dyadic interval
`[start, end)` of the coordinate space. Stores `own` and `sum`. Every G-node
also has a corresponding V-entry in the V-Tree.

**Why this name:** "G" stands for **Geometric** — the tree is a binary geometric
partition of `[0, 2^N)`.

---

## G-Tree

**Definition:** The binary spatial tree that partitions `[0, 2^N)` into
half-intervals. Routing is done by comparing a coordinate against midpoints.
Answers the question: _where is activity distributed?_

**Why this name:** "Geometric Tree" — the tree's structure is defined by the
geometry of the coordinate space (dyadic interval halvings), not by the order in
which items were inserted.

---

## V-entry

**Definition:** A G-node's "seat" in the V-Tree tournament bracket. Represents
the G-node's `own` intensity in the ranking. Every G-node has exactly one
V-entry; the two always exist and are destroyed together.

**Why this name:** "V" stands for **Value** — the V-Tree orders nodes by their
value (intensity). An "entry" is one participant in the tournament bracket.

**Examples:**

- When `[96, 128)` is created by a split, a new V-entry is inserted into the
  bracket for it.

---

## V-Tree

**Definition:** A tournament bracket over all G-nodes, ordered by `own`
intensity. Used to find the highest-intensity node quickly and to support
O(log N) weighted random sampling. Answers the question: _where is the hottest
activity, and how fast can I reach it?_

**Why this name:** "Value Tree" — it ranks nodes by their value (intensity),
unlike the G-Tree which ranks by spatial position.

---

## terminal

**Definition:** A G-node with no children. Receives observations directly (is
the routing target). Is a candidate for eviction.

**Why this name:** In tree terminology a node with no children is a _leaf_ or
_terminal node_. mudlark uses "terminal" throughout to avoid ambiguity with the
semi-internal state.

**Lifecycle:** `terminal` → (after first split) → `semi-internal` → (after second split) → `internal`. Reverse via eviction.

---

## semi-internal

**Definition:** A G-node with exactly one child. The child's half of the
interval routes observations to the child; the other half is still received
directly by this node.

**Why this name:** "Semi" because it is halfway between terminal (no children)
and internal (both children present).

---

## internal

**Definition:** A G-node with two children. Its `own` is permanently frozen;
all future observations are routed to a descendant. Cannot be evicted until at
least one child is evicted first.

**Why this name:** Standard tree terminology for a non-leaf node. An internal
node has children on both sides.

---

## plateau

**Definition:** A maximal contiguous span of the coordinate space where every
covering G-node sits at the same tree depth. The full index is always partitioned
into a set of plateaus.

**Why this name:** Imagine plotting the tree depth of the covering node as a
function of coordinate position — the graph looks like a step function. Each
flat horizontal segment is a "plateau". Where depth is uniform, resolution is
uniform; a step up means finer resolution; a step down means coarser resolution.

**Examples:**

```
Depth:  2  2  2  2  1  1  1
Coord:  0  32 64 96 128    256
         ← plateau A  →← B →
```

After `[0, 128)` has been split to depth 2 and `[128, 256)` remains at depth 1,
there are two plateaus.

---

## split

**Definition:** The operation that converts a terminal G-node into a
semi-internal or internal G-node by creating one or two children, halving its
interval. Triggered when `own` exceeds `split_threshold`.

---

## eviction

**Definition:** The operation that removes a terminal G-node, absorbs its `own`
into its parent, removes its V-entry, and triggers a V-Tree rebalance. Used to
enforce the memory budget by removing the deepest and coldest leaves first.

---

## streaming

**Definition:** Processing events one at a time, in arrival order, without
storing them or waiting to see the full dataset. After each `observe()` call the
raw event is discarded; only the aggregated tree state is retained.

**Why this name:** From the _streaming algorithms_ field — algorithms that make
a single pass over data and maintain only a compact summary (bounded memory,
no replay). Classic examples: HyperLogLog, Count-Min sketch. mudlark belongs to
this family.

**Contrast with batch processing:** a batch approach collects all events first
and then computes a histogram (e.g. a nightly log job). mudlark is always
up-to-date because it updates on every event; a flood that lasts 30 seconds is
visible immediately, not the next morning.

---

## decay

**Definition:** A global operation `decay(attenuation, q)` that multiplies all
`own` values by a factor ≤ 1, simulating a sliding time window. `q = 0` scales
all nodes equally; `q > 0` scales deeper nodes more aggressively.

---

## PEWEI

**Definition:** A significance-ordered export format produced by `extract()`.
Stands for **P**hase-**E**ncoded **W**eighted **E**ntropy **I**ndex (defined in
the mudlark ADRs). Layers are sorted from most- to least-significant, suitable
for external analysis or anomaly detection.
