//! UDP tracker: detect coordinated attacks by IP address range.
//!
//! Simulates a scenario where isolated bad-request counts look harmless,
//! but a coordinated /24 sweep is immediately visible as a hot range.
//!
//! See `docs/use-cases/ip-range-ban-detection.md` for the full design rationale.
//!
//! Run with:
//! ```bash
//! cargo run --example ip_range_ban_detection
//! ```

use std::net::Ipv4Addr;

use torrust_mudlark::{Config, GState, GvGraph};

// ---------------------------------------------------------------------------
// Graph type
// ---------------------------------------------------------------------------

/// Track bad UDP-tracker announce requests per source IPv4 address.
///
/// `N = 32` covers the full 32-bit IPv4 address space (`[0, 2^32 - 1]`).
/// For IPv6 monitoring use `GvGraph<u64, u64, 64>` and map the top 64 bits.
type BadRequestMap = GvGraph<u32, u64, 32>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert four IPv4 octets to a `u32` key in network byte order.
fn ipv4(a: u8, b: u8, c: u8, d: u8) -> u32 {
    u32::from(Ipv4Addr::new(a, b, c, d))
}

/// Minimal deterministic LCG — satisfies `torrust_mudlark::Rng` without
/// pulling in an external crate as a dev-dependency.
struct Lcg(u64);

impl torrust_mudlark::Rng for Lcg {
    #[allow(clippy::cast_precision_loss)]
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

/// Print a snapshot of every live node in the graph, ordered by BFS depth.
///
/// Each line shows:
///   `[depth] <start_ip ... end_ip>  own=N  sum=N  (state)`
///
/// - `own`   — value accumulated directly at this node (non-zero on terminal
///             nodes once the tree has split fine enough).
/// - `sum`   — total value in the subtree rooted here; equals `own` for leaves.
/// - `state` — `Terminal` (leaf), `Internal` (both children split),
///             or `SemiInternal` (one child split).
///
/// Nodes that straddle a query boundary but haven't been split yet are visible
/// here — that is the reason `range_sum` can undercount slightly.
fn print_tree(label: &str, graph: &BadRequestMap) {
    println!("\n── {label} (total_sum={}) ──", graph.total_sum());
    println!(
        "{:>5}  {:<45}  {:>10}  {:>10}  {}",
        "depth", "range [start_ip … end_ip]", "own", "sum", "state"
    );
    for (depth, node) in graph.layers() {
        let start_ip = Ipv4Addr::from(node.start);
        let end_ip = Ipv4Addr::from(node.end);
        let state = match node.state {
            GState::Terminal => "Terminal",
            GState::Internal => "Internal",
            GState::SemiInternal => "SemiInternal",
        };
        println!(
            "{:>5}  {:<21} … {:<21}  {:>10}  {:>10}  {}",
            depth,
            start_ip.to_string(),
            end_ip.to_string(),
            node.own,
            node.sum,
            state
        );
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let mut graph = BadRequestMap::new(Config {
        split_threshold: 5, // split once a sub-range accumulates ≥5 bad requests
        depth_create: 4,
        depth_evict: 16,
        // budget: None means unbounded for this demo.
        // In production set a hard cap; it must exceed 3^(depth_evict-depth_create+1).
        // E.g. depth_create=2, depth_evict=7 → 3^6=729 → budget: Some(1024).
        budget: None,
        alpha_relax: 0.5,
        bounded_eviction: false,
    });

    // -- 1. Background noise: a few isolated bad requests from separate IPs ---
    //
    // Each of these would trip a per-IP counter but nothing looks alarming as
    // a group.
    for &addr in &[
        ipv4(10, 0, 0, 1),
        ipv4(192, 168, 1, 42),
        ipv4(172, 16, 5, 100),
    ] {
        graph.observe(addr, 1_u64);
    }
    println!("After noise:  total bad requests = {}", graph.total_sum());
    // → 3
    print_tree("After noise", &graph);
    // Expect: 3 nodes — one coarse node per observed IP, everything else silent.

    // -- 2. Coordinated attack: every host in 203.0.113.0/24 -----------------
    //
    // Each IP sends only 20 bad requests — well below a typical per-IP
    // threshold of 100.  As a /24 block they accumulate 5 120 hits total.
    // A plain HashMap<IpAddr, u32> would see 256 small counters and fire no
    // alarm.  GvGraph sees a hot range.
    for host in 0_u8..=255 {
        for _ in 0..20 {
            graph.observe(ipv4(203, 0, 113, host), 1_u64);
        }
    }
    println!("After attack: total bad requests = {}", graph.total_sum());
    // → 5123  (3 noise + 5120 attack)
    print_tree("After attack", &graph);
    // Expect: the 203.0.113.0/24 block is split into many fine-grained nodes
    // (high depth, small ranges), while the three noise IPs remain coarse.
    // Nodes that straddle the /24 boundary are visible here — they explain
    // why range_sum returns ~4929 rather than the exact 5120.

    // -- 3. Range query: score the suspected /24 without enumerating hosts ----
    let subnet_lo = ipv4(203, 0, 113, 0);
    let subnet_hi = ipv4(203, 0, 113, 255);
    let subnet_score = graph.range_sum(subnet_lo..=subnet_hi);
    println!("203.0.113.0/24 score  = {subnet_score}  → ban the subnet");
    // → ~4929  (slightly under 5120 because the tree has not split to per-host
    //           granularity everywhere; still overwhelms the 3-point noise floor)

    // -- 4. Automatic triage: sample surfaces the hottest region --------------
    //
    // No need to enumerate candidates — the structure tells us where to look.
    let mut rng = Lcg(42);
    if let Some(cell) = graph.sample(&mut rng) {
        let suspect = Ipv4Addr::from(cell.start);
        println!("Highest-suspicion region starts at {suspect}");
        // → somewhere in 203.0.113.0/24  (high probability)
    }

    // -- 5. Temporal decay: expire old offenses after one hour ----------------
    //
    // Call decay on a timer so that recycled IP ranges are not permanently
    // penalised for a past attack. 0.5 attenuation halves all counts each
    // period; a four-hour-old attack retains only 6.25 % of its original score.
    let root = graph.g_root();
    graph.decay(root, 0.5, 0.001);
    println!("After decay:  total bad requests = {}", graph.total_sum());
    // → ~2487  (all counts halved; a new burst will still spike visibly)
    print_tree("After decay", &graph);
    // Expect: same tree shape — decay does not restructure the tree, only
    // scales all `own` and `sum` values down proportionally.
}
