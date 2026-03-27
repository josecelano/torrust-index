use torrust_mudlark::{Config, GNodeId, GvGraph, StructuralConfig};

// u32 coordinate, u64 accumulator, N=16 → address space [0, 2^16 - 1]
type TestGraph = GvGraph<u32, u64, 16>;

// -------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------

fn minimal_config() -> Config<u64> {
    Config {
        split_threshold: 10,
        structural: StructuralConfig {
            depth_create: 2,
            depth_evict: 5,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        },
    }
}

fn assert_no_violations(graph: &TestGraph) {
    let violations = torrust_mudlark::invariants::check_all_invariants(graph);
    assert!(
        violations.is_empty(),
        "Invariant violations ({} total):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A simple deterministic LCG used to satisfy the `Rng` bound without
/// pulling in an external crate as a dev-dependency.
struct DeterministicRng(u64);

impl torrust_mudlark::Rng for DeterministicRng {
    #[allow(clippy::cast_precision_loss)]
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

// -------------------------------------------------------------------
// Test 1 — new graph satisfies invariants
// -------------------------------------------------------------------

#[test]
fn new_graph_satisfies_invariants() {
    let graph = TestGraph::new(minimal_config());
    assert_no_violations(&graph);
}

// -------------------------------------------------------------------
// Test 2 — single observation satisfies invariants
// -------------------------------------------------------------------

#[test]
fn single_observe_satisfies_invariants() {
    let mut graph = TestGraph::new(minimal_config());
    graph.observe(1_000_u32, 5_u64);
    assert_no_violations(&graph);
}

// -------------------------------------------------------------------
// Test 3 — repeated observations on the same coord trigger splits
// -------------------------------------------------------------------

#[test]
fn repeated_observe_same_coord_satisfies_invariants() {
    let mut graph = TestGraph::new(minimal_config());
    for _ in 0..50 {
        graph.observe(1_000_u32, 1_u64);
    }
    assert_no_violations(&graph);
}

// -------------------------------------------------------------------
// Test 4 — observations spread across the space (full tree exercise)
// -------------------------------------------------------------------

#[test]
fn observe_many_coords_satisfies_invariants() {
    let mut graph = TestGraph::new(minimal_config());
    for i in 0_u32..200 {
        graph.observe(i.wrapping_mul(300) % 65_535, 1_u64);
    }
    assert_no_violations(&graph);
}

// -------------------------------------------------------------------
// Test 5 — get returns zero for an unobserved coordinate
// -------------------------------------------------------------------

#[test]
fn get_returns_zero_for_unobserved_coord() {
    let graph = TestGraph::new(minimal_config());
    let cell = graph.get(1_000_u32);
    assert_eq!(cell.intensity, 0_u64);
}

// -------------------------------------------------------------------
// Test 6 — total_sum reflects accumulated value after observe
//
// get().intensity returns the G-node's `own` field, which is only
// non-zero on terminal G-nodes after a split. total_sum() is the
// correct way to verify the graph has recorded the observation.
// -------------------------------------------------------------------

#[test]
fn total_sum_nonzero_after_observe() {
    let mut graph = TestGraph::new(minimal_config());
    graph.observe(1_000_u32, 42_u64);
    assert!(
        graph.total_sum() > 0,
        "expected non-zero total_sum after observe"
    );
}

// -------------------------------------------------------------------
// Test 7 — sample returns None on empty graph, Some after observations
// -------------------------------------------------------------------

#[test]
fn sample_returns_none_on_empty_graph() {
    let graph = TestGraph::new(minimal_config());
    let mut rng = DeterministicRng(42);
    let result = graph.sample(&mut rng);
    assert!(
        result.is_none(),
        "expected None on empty (zero-weight) graph"
    );
}

#[test]
fn sample_returns_some_after_observations() {
    let mut graph = TestGraph::new(minimal_config());
    for i in 0_u32..20 {
        graph.observe(i.wrapping_mul(3_000) % 65_535, 1_u64);
    }
    let mut rng = DeterministicRng(42);
    let result = graph.sample(&mut rng);
    assert!(result.is_some(), "expected Some after observations");
}

// -------------------------------------------------------------------
// Test 8 — decay satisfies invariants
// u64 implements Attenuatable so no type change needed
// -------------------------------------------------------------------

#[test]
fn decay_satisfies_invariants() {
    let mut graph = TestGraph::new(minimal_config());
    for i in 0_u32..20 {
        graph.observe(i.wrapping_mul(3_000) % 65_535, 5_u64);
    }
    let root: GNodeId = graph.g_root();
    graph.decay(root, 0.9, 0.01);
    assert_no_violations(&graph);
}

// -------------------------------------------------------------------
// Test 9 — bounded budget: invariants hold after many observations
// depth_create=2, depth_evict=4 → buffer=2 → required = max(27, 2) = 27
// budget must be > 27
// -------------------------------------------------------------------

#[test]
fn bounded_budget_satisfies_invariants() {
    let config: Config<u64> = Config {
        split_threshold: 1,
        structural: StructuralConfig {
            depth_create: 2,
            depth_evict: 4,
            budget: Some(50),
            alpha_relax: 0.5,
            bounded_eviction: true,
        },
    };
    let mut graph: GvGraph<u32, u64, 16> = GvGraph::new(config);
    for i in 0_u32..500 {
        graph.observe(i.wrapping_mul(127) % 65_535, 1_u64);
    }
    let violations = torrust_mudlark::invariants::check_all_invariants(&graph);
    assert!(
        violations.is_empty(),
        "Invariant violations ({} total):\n{}",
        violations.len(),
        violations.join("\n")
    );
}
