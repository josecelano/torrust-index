# `torrust-mudlark` — Design Alternatives

This document surveys data structures that could in principle replace or
approximate mudlark's dual-tree design, with an honest assessment of where each
one falls short.

The three operations that must all be fast are:

1. **`observe(coord, intensity)`** — update the index on every incoming event.
2. **`range_sum(start, end)`** — total intensity in an arbitrary coordinate range.
3. **`sample(&mut rng)`** — pick a random coordinate, weighted by intensity.
4. **Adaptive resolution** — the structure must zoom in on hot regions and evict
   cold ones to stay within a fixed memory budget.

Any design that satisfies all four in O(log N) time and O(budget) space is
functionally equivalent to mudlark. The question is how naturally the design
handles adaptive resolution — which is where most alternatives break down.

---

## Sorted linked list

**How it would work:** maintain a sorted list of `(interval, intensity)` pairs.
Each `observe` scans to the right cell and increments it. Splits extend the list
in-place.

**Why it doesn't work:**

- Every operation is O(N) — inserting, scanning for a coordinate, accumulating a
  range sum, and weighted sampling all require a full or partial traversal.
- For millions of events per hour, O(N) per event is unusable.

**Verdict:** ❌ too slow for all four operations.

---

## Sorted array + Fenwick tree (Binary Indexed Tree)

**How it would work:** keep intervals in a sorted array. Use a Fenwick tree over
the array positions for O(log N) prefix sums, enabling range queries and weighted
sampling.

**Why it almost works — but splits break it:**

- Point query, range sum, and weighted sample are all O(log N). ✅
- But when a hot node splits, the new child must be inserted into the middle of
  the array. This shifts all subsequent elements — O(N) per split. ❌
- With millions of splits over a long-running session, this is prohibitive.

**Verdict:** ⚠️ good for static or slowly-changing distributions; cannot handle
high split rates.

---

## Augmented balanced BST (e.g. AVL tree or red-black tree with subtree sums)

**How it would work:** a self-balancing BST keyed by interval start, with each
node augmented to cache its subtree's total intensity. Range queries and weighted
sampling walk the tree using the cached sums.

**Why it is close but not identical:**

- All four operations are O(log N). ✅
- Adaptive splits and evictions are O(log N). ✅
- Works correctly for arbitrary split points — the BST does not require dyadic
  intervals.

**What it trades away:**

- **Routing without metadata:** a balanced BST must store each split point and
  look it up at every level. mudlark's dyadic structure makes routing pure
  arithmetic — no stored split points needed.
- **Alignment with bit-string domains:** dyadic intervals align exactly with IP
  prefix notation, infohash prefixes, and peer ID prefixes. An arbitrary BST
  loses this property.
- **Formal analysis:** the φ-bounded growth properties proved in mudlark's ADRs
  rely on the dyadic structure. An arbitrary BST loses tractable worst-case bounds.

**Verdict:** ✅ viable general-purpose alternative; trades routing simplicity and
domain alignment for flexibility in split placement. If the coordinate domain
does not have natural bit-string structure, this may be preferable.

---

## Fixed-size segment tree

**How it would work:** allocate a complete binary segment tree over `[0, 2^N)`
at fixed depth D. Each leaf covers a fixed interval of width `2^(N-D)`.
Intensity is accumulated at the leaf; internal nodes cache subtree sums.

**Why it is conceptually the simplest design:**

- All operations are O(D) = O(log(2^N)) = O(N bits). ✅
- No splits, no evictions, no rebalancing. Very simple to implement.
- Range sum and weighted sample both use cached subtree sums. ✅

**Why it fails for mudlark's use case:**

- **Resolution is fixed.** You must choose D upfront. A hot region gets the same
  resolution as a cold one. If D is too small, hot regions are underresolved; if
  too large, cold regions waste memory.
- **Memory is O(2^D) regardless of activity.** For D=20 (1M leaves) you always
  allocate ~1M nodes even if only 10 coordinates are ever observed.
- **No memory budget.** The structure cannot shrink when budget is tight.

**Verdict:** ❌ unsuitable for adaptive, memory-bounded use cases. Good for
offline histogram computation over a known, bounded domain.

---

## Sparse/dynamic segment tree

**How it would work:** like a fixed segment tree, but only materialise a node
when its region receives its first observation. Unmaterialised nodes are
implicitly zero.

This is the closest known data structure to the G-Tree. Understanding where they
overlap — and where they diverge — is the clearest way to define what the G-Tree
actually is.

### What the G-Tree shares with a sparse segment tree

| Property       | Sparse segment tree                    | G-Tree                                      |
| -------------- | -------------------------------------- | ------------------------------------------- |
| Tree shape     | Binary, dyadic (midpoint splits)       | Binary, dyadic ✅ same                      |
| Node creation  | On first observation in region         | On first observation ✅ same                |
| Routing        | Pure arithmetic: left if `coord < mid` | Pure arithmetic ✅ same                     |
| Space coverage | Full domain, gaps are implicit zero    | Full domain, gaps are implicit zero ✅ same |
| Depth variance | Hot paths deeper, cold paths coarser   | Hot paths deeper ✅ same                    |
| Subtree sums   | Cached at each internal node           | Cached (`sum` field) ✅ same                |
| Range query    | O(log N)                               | O(log N) ✅ same                            |

A reader familiar with sparse segment trees can think of the G-Tree as **a
sparse segment tree with three extensions bolted on.**

### Where the G-Tree extends a sparse segment tree

**Extension 1 — intensity-triggered splits (not observation-triggered)**

A standard sparse segment tree creates a leaf on first observation and that leaf
never splits. The G-Tree also creates a leaf on first observation, but
subsequently splits that leaf into two children when its accumulated intensity
exceeds a threshold (φ). This is what gives the G-Tree adaptive resolution: a
hot region accumulates many splits and becomes fine-grained; a cold region stays
as a single coarse leaf.

**Extension 2 — eviction (the tree can shrink)**

A standard sparse segment tree can only grow — once a node exists, it stays
forever. The G-Tree can also shrink. When intensity decays and a subtree becomes
cold, and when the node count exceeds the memory budget, the cold subtree can be
evicted (collapsed back into its parent). This is what makes the node count
bounded and the structure self-adapting over time rather than permanently growing
toward the worst-case allocation.

**Extension 3 — decay (intensities cool off)**

A standard sparse segment tree accumulates intensity monotonically — every
observation adds to the sum and the sum never decreases. The G-Tree applies a
decay function so that old observations lose weight over time. Without decay,
every region that was ever hot remains hot forever, eviction never fires, and the
budget bound cannot be maintained.

### The V-Tree gap

Even with all three extensions, a sparse segment tree still cannot do O(log N)
weighted sampling. To sample a random coordinate weighted by intensity you would
need to walk the subtree-sum caches top-down, which is O(log N) — but only if
you additionally maintain a _ranked_ index of which subtrees have the most
intensity, so you can find the heaviest branch at each step without scanning all
siblings. That ranked index is exactly the V-Tree. You would independently
reinvent it.

### Summary

> The G-Tree = sparse segment tree
>
> - intensity-triggered splits
> - φ-bounded eviction
> - temporal decay
> - V-Tree pointers for O(log N) weighted sampling

**Verdict:** ✅ the G-Tree _is_ a sparse segment tree with adaptive resolution
and a memory budget. You still need a second structure (the V-Tree) for O(log N)
weighted sampling.

---

## Skip list

**How it would work:** a probabilistic sorted structure with O(log N) expected
time for insert, delete, and search.

**Why it doesn't fit naturally:**

- Skip lists are designed for point queries and sorted traversal, not range
  aggregation or weighted sampling.
- Adding subtree sums to a skip list is non-trivial and loses the probabilistic
  balance guarantees.
- Adaptive resolution (splitting an interval into two) requires inserting a new
  node and re-routing future observations — doable but awkward.

**Verdict:** ⚠️ not a natural fit; possible but requires significant augmentation
and loses the clean properties of BST-family structures.

---

## Count-Min sketch

**How it would work:** a probabilistic frequency table. Each `observe(coord,
intensity)` hashes the coordinate through several hash functions and increments
counters. Querying returns an upper bound on frequency.

**Why it is complementary rather than a replacement:**

- Excellent for top-K frequency queries over a large universe of discrete keys. ✅
- Extremely memory-efficient. ✅
- **No spatial structure.** Cannot answer range queries (`range_sum(100, 200)`).
- **No adaptive resolution.** All coordinates are treated equally; there is no
  concept of "zoom in on this hot region".
- **No weighted sampling.** Cannot implement `sample()` directly.
- Answers are approximate (probabilistic error bounds).

Count-Min sketch is a good complement to mudlark for exact-key frequency
questions (e.g. "how many times has this exact infohash been announced?") where
spatial proximity is irrelevant.

**Verdict:** ❌ different tool for a different job. Use alongside mudlark, not
instead of it.

---

## HyperLogLog

**How it would work:** estimates the _cardinality_ (number of distinct
coordinates observed), not the intensity distribution.

**Why it is irrelevant here:**

HyperLogLog answers "how many distinct infohashes have been seen?" — not "which
region is hottest?" or "what is the total intensity in range [100, 200]?".
Useful as a supplementary metric but not a substitute for any part of mudlark.

**Verdict:** ❌ answers a different question entirely.

---

## Eytzinger layout

**What it is:** a cache-friendly flat-array representation of a complete binary
tree. The children of node at index `i` are stored at `2i` and `2i+1`. This
packs the top levels of the tree into the same CPU cache lines, reducing cache
misses during top-down traversal.

**Why it is incompatible with the G-Tree:**

Eytzinger layout has one hard precondition: the full tree shape must be known
upfront and must never change. The G-Tree violates this in two ways:

1. **Dynamic splits.** When a node's intensity exceeds the φ threshold, it splits
   into two children at runtime. In an Eytzinger array, inserting a node in the
   middle of the layout shifts all subsequent elements — O(N) per split.

2. **Evictions leave holes.** When a cold subtree is evicted, its slots in the
   array become empty. You either waste the space (breaking the `2i`/`2i+1` index
   arithmetic) or compact the array (O(N) again).

Eytzinger shines for **read-heavy, static** trees — a tree you build once and
query billions of times. The cache-line-friendly traversal is a real win in that
setting, but requires a frozen structure.

The G-Tree is the opposite: write-heavy (every `observe` may split or decay),
mutable (evictions fire under budget pressure), and unpredictably shaped.
Pointer-based heap allocation is the right choice — each node can be created or
freed independently without touching its neighbours.

**Verdict:** ❌ incompatible with dynamic splits and evictions. Relevant only for
static, pre-built trees.

---

## Can the G-Tree be built on a standard segment tree library?

**Short answer:** only for the structural skeleton — not for the parts that make
mudlark useful.

A standard segment tree library (e.g. competitive-programming-style Rust crates
such as `segment_tree` or `fenwick`) gives you:

- A binary tree over `[0, 2^N)` with dyadic (midpoint) splits ✅
- O(log N) range aggregation (sum, min, max) ✅
- O(log N) point update ✅

But all standard libraries have a **fixed shape** allocated upfront. They cannot
do what the G-Tree actually does:

| Feature         | Standard seg-tree library      | G-Tree                                        |
| --------------- | ------------------------------ | --------------------------------------------- |
| Node allocation | All nodes upfront — O(2^depth) | On-demand — only when observations arrive     |
| Depth           | Fixed at construction          | Varies by coordinate intensity                |
| Splits          | None — structure is static     | Dynamic — when intensity exceeds threshold    |
| Eviction        | None                           | Budget-driven, φ-bounded cold-subtree removal |
| V-Tree pointers | None                           | Each node carries V-entry ownership           |

The **routing logic** (go left if `coord < midpoint`, go right otherwise) is
identical to a standard segment tree — the concept can be borrowed from one. But
dynamic node creation, intensity-triggered splits, and budget-driven eviction are
all custom to mudlark and have no off-the-shelf counterpart.

In practice, if you tried to build on a standard library:

- You would have to pre-allocate for the worst-case depth (32 levels for 32-bit
  coordinates = 4 billion nodes) — completely impractical.
- Or you would wrap it in a `HashMap` of active nodes, at which point you have
  reimplemented a sparse segment tree from scratch and still need to bolt on
  splits, eviction, and V-Tree integration yourself.

**Verdict:** use a standard segment tree library as a _mental model_ for the
routing logic; the full G-Tree must be implemented from scratch.

---

## Summary

| Structure              | Observe      | Range sum    | Weighted sample       | Adaptive resolution | Memory budget        |
| ---------------------- | ------------ | ------------ | --------------------- | ------------------- | -------------------- |
| Sorted linked list     | O(N)         | O(N)         | O(N)                  | Easy                | Easy                 |
| Sorted array + Fenwick | O(N) splits  | O(log N)     | O(log N)              | O(N) per split      | Hard                 |
| Augmented balanced BST | O(log N)     | O(log N)     | O(log N)              | O(log N)            | Possible             |
| Fixed segment tree     | O(log N)     | O(log N)     | O(log N)              | ❌ fixed            | ❌ fixed             |
| Sparse segment tree    | O(log N)     | O(log N)     | O(log N) needs V-Tree | O(log N)            | Possible             |
| Skip list              | O(log N)     | Complex      | Complex               | Awkward             | Possible             |
| Count-Min sketch       | O(1)         | ❌           | ❌                    | ❌                  | ✅ excellent         |
| **mudlark dual-tree**  | **O(log N)** | **O(log N)** | **O(log N)**          | **✅ dyadic**       | **✅ budget-driven** |

The sparse segment tree augmented with subtree intensity caches is the closest
alternative and converges to the same dual-tree design mudlark uses. The main
differentiator of mudlark is the φ-bounded eviction policy and the formal
analysis of its memory-budget behaviour, which are not properties of an
off-the-shelf sparse segment tree.
