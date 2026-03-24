# Use case: detecting coordinated attacks by IP range

> Runnable example: [`examples/ip_range_ban_detection.rs`](../../examples/ip_range_ban_detection.rs)
>
> ```bash
> cargo run --example ip_range_ban_detection
> ```

---

## Problem

The Torrust UDP tracker bans peers that repeatedly send announces with an invalid connection ID. The typical rule is: after N bad requests from the same IP (N=10 by default), ban that IP for one hour.

This works against naive single-IP offenders, but it has two blind spots:

**Blind spot 1 — coordinated sweeps**: an attacker using one IP per request never exceeds the threshold on any individual IP. If a /24 block sends 10 bad requests each, that is 2 560 violations with zero bans triggered.

**Blind spot 2 — IPv6 (/48 rotation)**: IPv6 address space is cheap. An attacker delegated a /48 block controls 2^80 unique addresses. A `HashMap<IpAddr, u32>` grows unboundedly and the threshold is never hit for any single address.

A plain per-IP counter can only see the trees; it cannot see the forest.

---

## Why `GvGraph` helps

`GvGraph` observes at the coordinate level (individual IP) but accumulates value across the address _space_. When a subnet becomes active it automatically splits its representation at finer granularity — the φ-bounded geometry concentrates resolution exactly where the activity is.

This gives three capabilities that a `HashMap` cannot provide:

| Capability                                      | `HashMap<IpAddr, u32>` |      `GvGraph`      |
| ----------------------------------------------- | :--------------------: | :-----------------: |
| Per-IP count                                    |           ✓            |          ✓          |
| Range query without enumerating IPs             |           ✗            | ✓ via `range_sum()` |
| Automatically surfaces the hottest sub-range    |           ✗            |  ✓ via `sample()`   |
| Temporal decay (sliding window, not cumulative) |         manual         |   ✓ via `decay()`   |
| Bounded memory regardless of IP diversity       |           ✗            |   ✓ via `budget`    |

---

## Design

### Coordinate encoding

Map IPv4 addresses to `u32` directly via `u32::from(Ipv4Addr::new(a, b, c, d))`. The full 32-bit IPv4 space maps cleanly to a `GvGraph<u32, u64, 32>`.

For IPv6, take the top 64 bits of the address as a `u64` coordinate. A `/48` prefix corresponds to a contiguous range in that 64-bit space, so range queries still work correctly.

### Parameters to tune

| Parameter         | Role                               | Guidance                                      |
| ----------------- | ---------------------------------- | --------------------------------------------- |
| `split_threshold` | minimum score to subdivide a node  | lower = finer early detection; start at 5–20  |
| `budget`          | maximum node count                 | 1 024–4 096 for a tracker; limits memory      |
| `depth_evict`     | maximum tree depth before eviction | 16–24 for IPv4; 24–32 for IPv6                |
| decay attenuation | fraction retained per decay call   | 0.5 = halved per call; tune to the ban window |

### Detection workflow

```
on each bad-request event:
  graph.observe(source_ip_as_u32, 1)

on ban-check timer (e.g. every 30 s):
  for each candidate subnet to audit:
    score = graph.range_sum(subnet_lo..=subnet_hi)
    if score > BAN_THRESHOLD:
      ban(subnet)

  // Surface the single highest-risk region without scanning all nodes:
  if let Some(cell) = graph.sample(&mut rng):
    audit(cell.start..=cell.end)

on hourly timer:
  root = graph.g_root()
  graph.decay(root, 0.5, 0.001)   // halve all counts; bans expire naturally
```

---

## Limitations and open questions

- **IPv6 top-64 mapping**: addresses where only the lower 64 bits differ map to the same coordinate. This is acceptable for range-based attack detection but means individual-host accuracy is lost for the bottom half of the address.
- **N parameter vs. depth**: with `N=32` the structure can theoretically split to single-address resolution, but in practice the `budget` cap prevents that. The effective resolution is determined by how many splits the budget allows.
- **Single-port vs. multi-port**: this example counts bad requests across all destination ports. A per-port instance would let you correlate an IP range that is probing a specific service port.
- **Decay cadence**: decay should match the intended ban duration. For a one-hour ban, one decay call per hour at 0.5 attenuation roughly halves the threat score each period, which means an attack from 4 hours ago retains 0.5^4 = 6.25% of its original weight.
