# `torrust-mudlark` — Motivating Problems and Use Cases

This document describes the concrete operational problems that drove the design of
mudlark, together with an assessment of how well the library fits each one.

> **Note:** mudlark is a _sensor_, not a firewall or policy engine. It tells you
> _where_ activity is anomalously concentrated; a separate policy layer (e.g.
> `torrust-sentinel`) makes the actual banning or rate-limiting decision.

The common thread across all cases: **the signal is in the shape of the
distribution, not in any individual value.** A single announce from `10.0.0.1`
is normal; ten thousand announces from the `10.0.0.0/16` block in 30 seconds is
not. mudlark makes that shape visible and queryable at low cost.

> **Coordinate engineering:** mudlark requires that nearby integers correspond
> to semantically related events. For domains that are not naturally integers
> (strings, multi-dimensional tuples, taxonomies) you need to engineer an
> encoding that preserves that locality. Techniques and examples are documented
> in [COORDINATE_ENGINEERING.md](COORDINATE_ENGINEERING.md).

---

## Problem 1 — Malformed announces from peers without a connection ID

**Context:** In the BitTorrent UDP tracker protocol, clients must first obtain a
connection ID before sending an announce. Peers that skip this step and send raw
announce requests are either buggy or deliberately abusive. The standard defence
is IP-based rate limiting — but per-IP counters for IPv6 are impractical: the
address space is so sparse that an attacker can cycle through millions of
addresses in a single /64 block and each one would be a new, cold counter.

**How mudlark fits: ✅ good fit**

Map the source IP to `[0, 2^N)` by taking the first N bits of the address.
mudlark tracks announce rates per IP-prefix bucket and adapts resolution
automatically: dense IPv4 ranges accumulate enough intensity to earn fine buckets;
sparse IPv6 ranges stay coarse. The policy layer then bans any prefix bucket
whose intensity exceeds a threshold, regardless of how many individual IPs are
hiding inside.

| Axis                 | Coordinate                     | Intensity                | Hot region means                                 |
| -------------------- | ------------------------------ | ------------------------ | ------------------------------------------------ |
| Source IP (v4 or v6) | First N bits of the IP address | 1 per malformed announce | That IP prefix block is abusing the UDP endpoint |

**Limitation:** IPv4 and IPv6 live in different address spaces and require
separate mudlark instances.

---

## Problem 2 — Password-reset flooding from the same IP block

**Context:** An attacker distributes a credential-stuffing or password-reset
brute-force attack across many IP addresses in the same network block (e.g.
a rented /24 or a residential ISP range). Per-IP counters miss the pattern
entirely; you need to see the _region_ heating up.

**How mudlark fits: ✅ good fit**

Same approach as Problem 1: use the source IP's first N bits as the coordinate.
mudlark will automatically zoom in on the hot /24 block while leaving the rest
of the IP space coarse. The policy layer can then rate-limit or challenge all
requests from that prefix.

| Axis      | Coordinate                    | Intensity                    | Hot region means                                   |
| --------- | ----------------------------- | ---------------------------- | -------------------------------------------------- |
| Source IP | First N bits of the source IP | 1 per password-reset request | That IP block is running a distributed brute-force |

---

## Problem 3 — Password-reset flooding keyed by email domain

**Context:** Same attack as Problem 2 but the attacker routes requests through
many different IP addresses (e.g. a botnet) so no IP block heats up. The common
signal is the target email domain: thousands of reset attempts all targeting
accounts at a single provider (`@gmail.com`), or spread across a whole domain
family (`@*.google.com`).

**The naive encoding breaks the model: hashing is wrong**

The first instinct is to hash domains to `[0, 2^N)`. This is a mistake: hash
functions deliberately destroy proximity — `gmail.com` and `gmai1.com` hash to
completely unrelated positions. mudlark's value is that nearby coordinates are
genuinely near each other; with hashed keys, the spatial structure is meaningless
and mudlark degrades to a slow approximate HashMap.

**The encoding that works: reverse domain notation**

Reverse the domain hierarchy before encoding:

```
gmail.com    →  com.gmail
yahoo.co.uk  →  uk.co.yahoo
mail.google.com  →  com.google.mail
```

Then encode the reversed string as a big-endian integer (one byte per character).
Now the numeric ordering matches the domain hierarchy:

- All `.com` domains cluster in the same broad region.
- All `google.com` sub-domains cluster in a sub-region of that.
- Domains that differ only in the last label are numerically adjacent.

This is the same principle used by DNS tries. mudlark sees it as a genuine
spatial signal, not random noise.

**How mudlark fits — depends on the attack pattern:**

| Attack pattern                                                 | Encoding             | mudlark fit                                           |
| -------------------------------------------------------------- | -------------------- | ----------------------------------------------------- |
| Many resets targeting one exact domain (`@gmail.com`)          | reverse + big-endian | ⚠️ one fixed coordinate — plain counter is simpler    |
| Many resets spread across a domain family (`@*.google.com`)    | reverse + big-endian | ✅ the `com.google` namespace region heats up         |
| Botnet using many unrelated domains across the whole namespace | any                  | ❌ no spatial signal — use a Count-Min sketch instead |

**Axes:**

| Axis                                | Coordinate                       | Intensity                    | Hot region means                                                               |
| ----------------------------------- | -------------------------------- | ---------------------------- | ------------------------------------------------------------------------------ |
| Email domain (reversed, big-endian) | `encode("com.gmail")` as integer | 1 per password-reset request | That domain namespace is being targeted — likely botnet or credential-stuffing |

**When to use a plain counter instead:** if the attacker always targets the same
single domain, the "region" degenerates to one point and a `HashMap<domain,
count>` is both simpler and cheaper. mudlark adds value only when the attack
spans a _neighbourhood_ of the domain space.

---

## Problem 4 — Fake-torrent DDoS via infohash neighbourhood flooding

**Context:** Attackers submit large numbers of torrents whose infohashes differ
by only a few bits — for example, only the last hex digit changes. This clusters
them in a narrow region of the infohash space. The goal is to flood the index
with synthetic content, exhaust storage, or trigger denial-of-service behaviour
in range-based queries. Legitimate infohashes are spread roughly uniformly across
the full 160-bit space; a sudden high-density cluster in a narrow prefix bucket
is a strong indicator of synthetic content.

**How mudlark fits: ✅ excellent fit**

Infohashes are already meaningful N-bit integers. Bit-adjacent fake infohashes
_literally land in the same narrow region_ of the coordinate space — no encoding
trick is needed. mudlark will automatically zoom in on that prefix bucket
(successive splits driven by the accumulated intensity), making the cluster
immediately visible. The policy layer can then reject or quarantine any infohash
whose prefix is already in a hot bucket above a density threshold.

| Axis     | Coordinate                   | Intensity                              | Hot region means                                                                         |
| -------- | ---------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------- |
| Infohash | First N bits of the infohash | 1 per torrent submission (or announce) | That infohash prefix neighbourhood contains suspiciously many entries — likely synthetic |

**Why this is the strongest fit:** unlike IP addresses (where proximity is a
network-topology accident) or email domains (where proximity is destroyed by
hashing), bit-adjacent infohashes are _semantically related_ — they were
deliberately constructed to cluster. mudlark's spatial resolution directly
exposes that construction.

---

## Problem 5 — Peer ID anomaly detection (buggy or misbehaving clients)

**Context:** Every BitTorrent client includes a self-identifying prefix in its
peer ID. The most common format is the Azureus style: `-XX1234-<12 random bytes>`
where `XX` is a two-letter client code and `1234` is the version number. Examples:

```
-TR3000-  →  Transmission 3.0.0
-lt1219-  →  libtorrent 1.2.19
-qB5200-  →  qBittorrent 5.2.0
```

A newly released client with a bug — for example, one that re-announces every
5 seconds instead of every 30 minutes, or one that sends duplicate announces on
reconnect — will generate a sudden spike in announce traffic from that client
family. Per-peer-ID counters detect individual offenders, but mudlark detects
the _client family as a whole_ heating up, which is the right signal when the
bug affects every user of a new release simultaneously.

This is distinct from an attack: the intent is benign, but the effect on tracker
load is the same. Distinguishing "new buggy release" from "coordinated flood"
is easier when you can see _which_ client prefix is hot.

**How mudlark fits: ✅ good fit for anomaly detection**

Encode the first N bytes of the peer ID as a big-endian integer. All peers from
the same client software share the same high-order bytes and cluster in the same
bucket. mudlark will zoom in on that bucket automatically when traffic spikes.

| Axis           | Coordinate                                   | Intensity      | Hot region means                                                       |
| -------------- | -------------------------------------------- | -------------- | ---------------------------------------------------------------------- |
| Peer ID prefix | First N bytes of peer ID as big-endian `u64` | 1 per announce | That client family is announcing far more than expected — bug or flood |

**What mudlark tells you vs. what it does not:**

| Signal                                   | mudlark can answer                                               | Needs separate logic                                                                                                 |
| ---------------------------------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Which client family is anomalously busy? | ✅ hot bucket prefix                                             | —                                                                                                                    |
| Is it a bug or an attack?                | ❌                                                               | Compare peer ID prefix to known client registry; check whether source IPs are diverse (bug) or concentrated (attack) |
| Which specific version is affected?      | ⚠️ only if the hot bucket resolves to single-version granularity | May need finer N                                                                                                     |

**Limitation — peer IDs are self-reported:** they are easy to spoof. An attacker
can impersonate any client family. mudlark over peer IDs is most useful as a
_diagnostic_ tool for detecting real client bugs rather than as a security
enforcement mechanism. For security enforcement, combine with IP-prefix tracking
(Problems 1 and 2).

**Limitation — non-standard peer IDs:** the Azureus format is a convention, not
a protocol requirement. Clients using random or non-standard peer IDs will not
cluster and will appear as background noise.
