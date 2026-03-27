//! Micro-benchmark for `v_depth` contribution to the observe hot path.
//!
//! We measure `GvGraph::observe()` throughput on a steady-state graph to
//! capture the real-world contribution of depth lookups to the overall
//! algorithm.  The same bench is run again after Step 6.2a removes
//! `VNode::cached_depth` to quantify the regression (if any).
//!
//! Baseline numbers are recorded in `docs/refactoring/phase-6.md` (Step 6.1).
//!
//! # Running
//!
//! ```bash
//! cargo bench --bench depth
//! ```

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use torrust_mudlark::{Config, GvGraph, StructuralConfig};

type BenchGraph = GvGraph<u8, u32, 8>;

fn make_steady_state_graph() -> BenchGraph {
    // N=8: domain [0, 256).  depth_create=3, depth_evict=5, budget=128 keeps
    // the tree bounded so the bench runs in O(1) amortised per observe.
    let mut g = BenchGraph::new(Config {
        split_threshold: 3,
        structural: StructuralConfig {
            depth_create: 3,
            depth_evict: 5,
            budget: Some(128),
            alpha_relax: 0.5,
            bounded_eviction: false,
        },
    });

    // Warm-up: cycle through addresses to fill & stabilise the tree.
    let mut coord: u8 = 0;
    for _ in 0..512 {
        g.observe(coord, 1_u32);
        coord = coord.wrapping_add(17);
    }

    g
}

fn bench_observe_steady_state(c: &mut Criterion) {
    let mut g = make_steady_state_graph();
    let mut coord: u8 = 0;

    c.bench_function("observe/steady_state", |b| {
        b.iter(|| {
            g.observe(black_box(coord), black_box(1_u32));
            coord = coord.wrapping_add(7);
        });
    });
}

fn bench_observe_split_heavy(c: &mut Criterion) {
    // Small budget = frequent splits & evictions = frequent depth invalidation,
    // which exercises the stale-depth re-walk path the most.
    // buffer = depth_evict - depth_create = 4 - 2 = 2; headroom = 3^3 = 27;
    // budget must be > 27, so use 32.
    let mut g = BenchGraph::new(Config {
        split_threshold: 2,
        structural: StructuralConfig {
            depth_create: 2,
            depth_evict: 4,
            budget: Some(32),
            alpha_relax: 0.5,
            bounded_eviction: false,
        },
    });

    let mut coord: u8 = 0;
    // Warm up.
    for _ in 0..64 {
        g.observe(coord, 1_u32);
        coord = coord.wrapping_add(13);
    }

    c.bench_function("observe/split_heavy", |b| {
        b.iter(|| {
            g.observe(black_box(coord), black_box(1_u32));
            coord = coord.wrapping_add(7);
        });
    });
}

criterion_group!(
    benches,
    bench_observe_steady_state,
    bench_observe_split_heavy
);
criterion_main!(benches);
