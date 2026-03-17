# Coordinate Engineering for `torrust-mudlark`

mudlark requires exactly one thing from its input: **nearby integers must
correspond to semantically related events**. The library itself is agnostic to
how you produce the coordinate — the entire surface area for extension is in the
`encode(event) → u64` function the caller writes before calling `observe()`.

This document catalogues techniques for engineering good coordinates from
non-integer domains, so that mudlark's spatial resolution exposes meaningful
structure rather than noise.

> **The golden rule:** if two events should be treated as "neighbours" by the
> anomaly detector, their coordinates must be numerically close. If they should
> be unrelated, their coordinates must be numerically far apart. Encoding quality
> determines detection quality — mudlark rewards any improvement to the encoding
> function directly.

---

## Technique 1 — Direct prefix truncation (integers and byte strings)

**Use when:** the raw domain is already a meaningful integer or byte sequence
where the most significant bits carry the most structural information.

**Method:** take the first N bits of the value as a `u64`.

**Works for:** IPv4/IPv6 addresses, infohashes, peer IDs, cryptographic
identifiers, timestamp bins.

**Example:**

```
IPv4  10.20.30.40  →  0x0A14'1E28  (32-bit integer, first 16 bits = 0x0A14)
SHA1  a3f9...      →  first 32 bits as u64
```

**Why it works:** the MSBs encode the coarse grouping (network block, infohash
prefix, client family); the LSBs encode fine-grain identity. mudlark's dyadic
splits naturally follow this hierarchy.

---

## Technique 2 — Reverse-hierarchical encoding (trees and taxonomies)

**Use when:** the domain has a named hierarchy — domain names, file paths,
category trees, organisational structures.

**Method:** reverse the hierarchy so the most general label comes first, then
encode the resulting string as a big-endian integer (one byte per character).

**Example:**

```
gmail.com        →  com.gmail        →  0x636F6D2E676D61696C (big-endian bytes)
mail.google.com  →  com.google.mail  →  numerically close to com.google.maps
/usr/local/bin   →  bin.local.usr    →  (path hierarchy)
```

**Why it works:** the numeric ordering now mirrors the hierarchical grouping.
All `.com` domains share the same high-order bytes; all `google.com` sub-domains
share a longer common prefix. mudlark splits follow the taxonomy automatically.

**Works for:** email domains, DNS names, URL paths, package namespaces, torrent
category trees.

---

## Technique 3 — Space-filling curves (multi-dimensional inputs)

**Use when:** the domain is naturally 2D or N-dimensional and you want to detect
anomalies that are concentrated in a region of that N-D space.

**Method:** apply a Hilbert curve (or Z-order / Morton code) to map N dimensions
to one integer while preserving locality — points that are close in N-D remain
close on the curve.

**Example — (IP, port) pair:**

```
(10.0.0.1, 6881)  →  Hilbert(ip_bits, port_bits)  →  u64
(10.0.0.2, 6882)  →  numerically adjacent to above
(200.1.1.1, 80)   →  numerically far from above
```

**Why it works:** a Morton code interleaves the bits of each dimension;
a Hilbert curve provides even stronger locality. mudlark then detects attacks
that are concentrated in a 2D neighbourhood (IP block AND port range) rather
than requiring two separate single-axis indexes.

**Works for:** (IP, port) pairs, (lat, lon) geographic coordinates, (timestamp,
IP) temporal-spatial pairs, any product of two bounded integer ranges.

**Libraries:** `hilbert` crate in Rust; Morton encoding is a few lines of
bit-interleave code.

---

## Technique 4 — Locality-Sensitive Hashing (LSH) for strings

**Use when:** the domain consists of strings that are _semantically similar_
when they look similar (not when they are equal), and you want nearby-looking
strings to cluster.

**Method:** apply a min-hash or simhash scheme that maps a string to an integer
such that similar strings (by edit distance or Jaccard similarity) hash to
similar integers, with high probability.

**Example — user-agent strings:**

```
"libtorrent/1.2.18"   →  0x3A7F...
"libtorrent/1.2.19"   →  0x3A7F... (very close — one character changed)
"Transmission/3.00"   →  0xC219... (far — different client family)
```

**Why it works:** attackers modifying a single byte of their user-agent to
evade per-UA rate limiting will still land in the same mudlark bucket.

**Limitation:** LSH is probabilistic; there will be false neighbours and false
strangers. Tune the hash parameters to control the trade-off. This is more
complex to implement correctly than the other techniques.

**Works for:** user-agent strings, torrent names, peer IDs, HTTP headers.

---

## Technique 5 — Time binning (temporal coordinates)

**Use when:** you want to detect _when_ anomalies happen, not just where.

**Method:** divide time into fixed bins (e.g. 1-second, 1-minute, 5-minute
windows) and encode the bin index as the coordinate. Each `observe()` call
carries the current time bin as the coordinate and the event count as intensity.

**Example:**

```
Unix timestamp 1_741_000_060  →  bin index 60  (if bin size = 1_741_000_000 + 60s)
```

**Why it works:** mudlark will zoom in on time periods with high event density
and stay coarse during quiet periods. The `decay()` operation can be used to
age out old bins.

**Works for:** request rate over time, announce bursts, time-of-day anomaly
detection.

---

## Technique 6 — ML embeddings projected to 1D (advanced / offline)

**Use when:** the domain has rich structure that none of the above techniques
capture, and you have offline training data.

**Method:**

1. Train a vector embedding (Word2Vec, BERT fine-tuned on log data, etc.) that
   places semantically similar events near each other in N-D space.
2. Project the N-D embedding to 1D using PCA, UMAP, or a learned rank ordering.
3. Use the projected scalar as the mudlark coordinate.

**Example — torrent names:**

```
"Ubuntu 22.04 LTS" → embedding → project to 1D → 0.73 → (u64)(0.73 × 2^32)
"Ubuntu 22.10"     → nearby in 1D
"[FAKE] Ubuntu..."  → if fake torrents form a cluster in embed space, also nearby
```

**Why it works:** the 1D projection preserves the gross structure of the
embedding neighbourhood. mudlark's adaptive resolution then provides cheap
online tracking of that structure.

**Limitation:** this is an offline preprocessing step. The coordinate mapping is
fixed at training time; it cannot adapt online as attack patterns evolve. Re-
training requires restarting the mudlark instance with new coordinates. Use this
only when stable semantic structure exists and can be pre-learned.

---

## Summary

| Technique                             | Domain type                    | Locality preservation       | Complexity  |
| ------------------------------------- | ------------------------------ | --------------------------- | ----------- |
| Direct prefix truncation              | Integers, byte strings         | Exact                       | Trivial     |
| Reverse-hierarchical encoding         | Named hierarchies (DNS, paths) | Exact within hierarchy      | Low         |
| Space-filling curves (Hilbert/Morton) | Multi-dimensional integers     | Approximate (very good)     | Low–Medium  |
| Locality-Sensitive Hashing            | Arbitrary strings              | Probabilistic               | Medium–High |
| Time binning                          | Temporal streams               | Exact within bin resolution | Trivial     |
| ML embeddings → 1D projection         | Rich unstructured data         | Approximate (offline)       | High        |

The most impactful improvements usually come from picking the right technique
for the domain, not from tuning mudlark's internal parameters (split threshold,
budget, decay rate). A well-engineered coordinate makes everything else cheaper.
