use crate::arena::Arena;
use crate::graph::algorithm::plateau::noop_tracker::NoopPlateauTracker;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::{GNode, GNodeChildren};
use crate::nodes::vnode::VNode;
use crate::spatial::node::Node;
use crate::traits::{Accumulator, Coordinate, PlateauTracking};
use crate::tree::gtree::GTree;
use crate::tree::vtree::VTree;

use super::config::Config;

/// Feature-conditional default tracker type.
///
/// When `dynamic-contour-tracking` is enabled this resolves to
/// [`DynamicPlateauTracker`]; otherwise it resolves to [`NoopPlateauTracker`].
#[cfg(feature = "dynamic-contour-tracking")]
pub(crate) type DefaultTracker<C, V> =
    crate::graph::algorithm::plateau::DynamicPlateauTracker<C, V>;
#[cfg(not(feature = "dynamic-contour-tracking"))]
pub(crate) type DefaultTracker<C, V> = crate::graph::algorithm::plateau::NoopPlateauTracker;

/// Convenience type alias that selects the right tracker automatically.
///
/// External code that does not need to name the tracker explicitly should prefer
/// `DefaultGraph` over `GvGraph<C, V, N, SomeTracker>`.
#[cfg(feature = "dynamic-contour-tracking")]
pub type DefaultGraph<C, V, const N: u32> =
    GvGraph<C, V, N, crate::graph::algorithm::plateau::DynamicPlateauTracker<C, V>>;
#[cfg(not(feature = "dynamic-contour-tracking"))]
pub type DefaultGraph<C, V, const N: u32> =
    GvGraph<C, V, N, crate::graph::algorithm::plateau::NoopPlateauTracker>;

/// The `GvGraph` composite structure.
///
/// The fourth type parameter `T` selects the plateau-tracking strategy.
/// Leaving it unspecified uses `DefaultTracker<C, V>`, which is
/// `DynamicPlateauTracker<C, V>` when the `dynamic-contour-tracking` Cargo
/// feature is enabled and `NoopPlateauTracker` otherwise.
pub struct GvGraph<
    C: Coordinate,
    V: Accumulator,
    const N: u32,
    T: PlateauTracking<C, V> = DefaultTracker<C, V>,
> {
    pub(crate) gtree: GTree<C, V, N>,

    pub(crate) vtree: VTree<V>,

    pub(crate) config: Config<V>,

    /// The plateau-tracking strategy for this graph instance.
    pub(crate) tracker: T,
}

impl<
    C: Coordinate,
    V: Accumulator,
    const N: u32,
    T: PlateauTracking<C, V> + std::fmt::Debug + Clone,
> std::fmt::Debug for GvGraph<C, V, N, T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GvGraph")
            .field("gtree", &self.gtree)
            .field("vtree", &self.vtree)
            .field("config", &self.config)
            .field("tracker", &self.tracker)
            .finish()
    }
}

impl<C: Coordinate, V: Accumulator, const N: u32, T: PlateauTracking<C, V> + Clone> Clone
    for GvGraph<C, V, N, T>
{
    fn clone(&self) -> Self {
        Self {
            gtree: self.gtree.clone(),
            vtree: self.vtree.clone(),
            config: self.config.clone(),
            tracker: self.tracker.clone(),
        }
    }
}

/// Shared constructor helper — builds all tree structures except the tracker.
///
/// Returns `(gtree, vtree, config, g_root)`.
fn build_core<C: Coordinate, V: Accumulator, const N: u32>(
    config: Config<V>,
) -> (GTree<C, V, N>, VTree<V>, Config<V>, GNodeId) {
    const { assert!(N <= C::BITS, "N must be <= C::BITS") };
    config.validate();

    let depth_buffer = config.structural.depth_evict - config.structural.depth_create;
    let headroom_fanout = 3usize.pow(depth_buffer + 1);
    let headroom_convergence = 2 * (config.structural.depth_create as usize).saturating_sub(1);
    let headroom = headroom_fanout.max(headroom_convergence);
    let soft_limit = config.structural.budget.map(|b| {
        let s = b - headroom;
        assert!(
            s >= 1,
            "soft_limit must be >= 1 (budget={b}, headroom={headroom})"
        );
        s
    });

    let mut gnodes = Arena::new();
    let root_gnode = GNode::new_leaf(C::zero(), C::domain_max(N), V::zero(), None);
    let g_root = GNodeId::from_index(gnodes.alloc(root_gnode));

    let mut vnodes = Arena::new();
    let root_entry = VNode::new_entry(V::zero(), None, g_root, true, true);
    let v_root_id = VNodeId::from_index(vnodes.alloc(root_entry));
    gnodes.get_mut(g_root.index()).assign_entry(v_root_id);

    let gtree = GTree {
        nodes: gnodes,
        root: g_root,
        node_count: 1,
        terminal_count: 1,
        live_depth_evict: config.structural.depth_evict,
        live_depth_create: config.structural.depth_create,
        depth_buffer,
        headroom,
        soft_limit,
    };

    let vtree = VTree {
        nodes: vnodes,
        root: Some(v_root_id),
        violations: Vec::new(),
    };

    (gtree, vtree, config, g_root)
}

// ── Constructor for NoopPlateauTracker ────────────────────────────────────────

impl<C: Coordinate, V: Accumulator, const N: u32> GvGraph<C, V, N, NoopPlateauTracker> {
    /// Creates a new graph using the `NoopPlateauTracker` (no plateau mirror).
    ///
    /// This is the default constructor when the `dynamic-contour-tracking`
    /// feature is **disabled**.
    #[cfg(not(feature = "dynamic-contour-tracking"))]
    #[must_use]
    pub fn new(config: Config<V>) -> Self {
        let (gtree, vtree, config, _g_root) = build_core::<C, V, N>(config);
        Self {
            gtree,
            vtree,
            config,
            tracker: NoopPlateauTracker,
        }
    }
}

// ── Constructor for DynamicPlateauTracker ─────────────────────────────────────

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator, const N: u32>
    GvGraph<C, V, N, crate::graph::algorithm::plateau::DynamicPlateauTracker<C, V>>
{
    /// Creates a new graph using the `DynamicPlateauTracker` (incremental
    /// plateau mirror).
    ///
    /// This is the default constructor when the `dynamic-contour-tracking`
    /// feature is **enabled**.
    #[must_use]
    pub fn new(config: Config<V>) -> Self {
        use crate::graph::algorithm::plateau::DynamicPlateauTracker;
        use crate::spatial::plateau::BasisEdge;
        use crate::tree::gtree::GTree;

        let (gtree, vtree, config, g_root) = build_core::<C, V, N>(config);

        let root_key = BasisEdge(C::zero());
        let root_depth = GTree::<C, V, N>::depth_of_interval(C::zero(), C::domain_max(N));
        let tracker = DynamicPlateauTracker::with_root(
            root_key,
            root_depth,
            C::zero(),
            C::domain_max(N),
            g_root,
            N,
        );

        Self {
            gtree,
            vtree,
            config,
            tracker,
        }
    }
}

// ── Generic impl for all trackers ─────────────────────────────────────────────

impl<C: Coordinate, V: Accumulator, const N: u32, T: PlateauTracking<C, V>> GvGraph<C, V, N, T> {
    // ── Accessors ──────────────────────────────────────────────────────
    #[must_use]
    #[inline]
    pub const fn node_count(&self) -> u32 {
        self.gtree.node_count
    }

    #[must_use]
    #[inline]
    pub const fn terminal_count(&self) -> u32 {
        self.gtree.terminal_count
    }

    #[must_use]
    #[inline]
    pub const fn config(&self) -> &Config<V> {
        &self.config
    }

    #[must_use]
    #[inline]
    pub const fn budget(&self) -> Option<usize> {
        self.config.structural.budget
    }

    #[must_use]
    #[inline]
    pub const fn g_root(&self) -> GNodeId {
        self.gtree.root
    }

    #[must_use]
    #[inline]
    pub(crate) const fn v_root(&self) -> Option<VNodeId> {
        self.vtree.root
    }

    #[must_use]
    #[inline]
    pub fn total_sum(&self) -> V {
        self.gtree.nodes.get(self.gtree.root.index()).sum()
    }

    #[must_use]
    #[inline]
    pub(crate) const fn vnodes(&self) -> &Arena<VNode<V>> {
        &self.vtree.nodes
    }

    #[cfg(test)]
    #[must_use]
    #[inline]
    pub(crate) fn has_pending_violations(&self) -> bool {
        !self.vtree.violations.is_empty()
    }

    #[must_use]
    #[inline]
    pub const fn depth_evict(&self) -> u32 {
        self.gtree.live_depth_evict
    }

    #[must_use]
    #[inline]
    pub const fn depth_create(&self) -> u32 {
        self.gtree.live_depth_create
    }

    #[must_use]
    #[inline]
    pub const fn depth_buffer(&self) -> u32 {
        self.gtree.depth_buffer
    }

    #[must_use]
    #[inline]
    pub const fn headroom(&self) -> usize {
        self.gtree.headroom
    }

    #[allow(clippy::doc_markdown)]
    #[must_use]
    #[inline]
    pub const fn soft_limit(&self) -> Option<usize> {
        self.gtree.soft_limit
    }

    #[must_use]
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn gnode_depth(&self, gid: GNodeId) -> u32 {
        let g = self.gtree.nodes.get(gid.index());
        GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi())
    }

    // ── Queries ────────────────────────────────────────────────────────
    #[must_use]
    pub fn gnode_info(&self, id: GNodeId) -> Option<Node<C, V>> {
        if !self.gtree.nodes.is_occupied(id.index()) {
            return None;
        }
        let g = self.gtree.nodes.get(id.index());
        Some(Node {
            start: g.lo(),
            end: g.hi(),
            own: g.own(),
            sum: g.sum(),
            depth: GTree::<C, V, N>::depth_of_interval(g.lo(), g.hi()),
            state: g.state(),
            gnode_id: id,
            parent: g.parent(),
        })
    }

    #[doc(hidden)]
    #[must_use]
    pub fn gnode_children(&self, id: GNodeId) -> Option<GNodeChildren> {
        if !self.gtree.nodes.is_occupied(id.index()) {
            return None;
        }
        let g = self.gtree.nodes.get(id.index());
        Some(GNodeChildren {
            left: g.left(),
            right: g.right(),
        })
    }

    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: GNodeId, descendant: GNodeId) -> bool {
        if !self.gtree.nodes.is_occupied(ancestor.index())
            || !self.gtree.nodes.is_occupied(descendant.index())
        {
            return false;
        }
        let a = self.gtree.nodes.get(ancestor.index());
        let d = self.gtree.nodes.get(descendant.index());
        a.lo() <= d.lo() && a.hi() >= d.hi() && (a.lo() != d.lo() || a.hi() != d.hi())
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph, StructuralConfig};

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            structural: StructuralConfig {
                depth_create: 3,
                depth_evict: 5,
                budget: None,
                alpha_relax: 0.5,
                bounded_eviction: false,
            },
        }
    }

    // ── GvGraph::new ─────────────────────────────────────────────────────
    mod new {
        use super::*;

        #[test]
        fn starts_with_one_node() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.gtree.node_count, 1);
        }

        #[test]
        fn starts_with_one_terminal() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.gtree.terminal_count, 1);
        }

        #[test]
        fn starts_with_zero_sum() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.total_sum(), 0u32);
        }
    }

    // ── GvGraph::gnode_info ───────────────────────────────────────────────
    mod gnode_info {
        use super::*;

        #[test]
        fn returns_some_for_root() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(g.gnode_info(g.gtree.root).is_some());
        }

        #[test]
        fn root_covers_full_domain() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.gtree.root).unwrap();
            use crate::traits::Coordinate;
            assert_eq!(info.start, u8::zero());
            assert_eq!(info.end, u8::domain_max(8));
        }

        #[test]
        fn root_is_terminal_at_start() {
            use crate::nodes::gnode::GState;
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.gtree.root).unwrap();
            assert_eq!(info.state, GState::Terminal);
        }

        #[test]
        fn root_is_root_node() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.gtree.root).unwrap();
            assert!(info.is_root());
        }

        #[test]
        fn returns_none_for_unoccupied_index() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            use crate::handle::GNodeId;
            // index 999 is well beyond the one allocated gnode
            let unoccupied = GNodeId::from_index(999);
            assert!(g.gnode_info(unoccupied).is_none());
        }
    }

    // ── GvGraph::gnode_children ───────────────────────────────────────────
    mod gnode_children {
        use super::*;

        #[test]
        fn root_has_no_children_initially() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let ch = g.gnode_children(g.gtree.root).unwrap();
            assert!(ch.left.is_none());
            assert!(ch.right.is_none());
        }

        #[test]
        fn returns_none_for_unoccupied_index() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            use crate::handle::GNodeId;
            // index 999 is well beyond the one allocated gnode
            let unoccupied = GNodeId::from_index(999);
            assert!(g.gnode_children(unoccupied).is_none());
        }
    }

    // ── GvGraph::is_ancestor_of ───────────────────────────────────────────
    mod is_ancestor_of {
        use super::*;

        #[test]
        fn node_is_not_ancestor_of_itself() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(!g.is_ancestor_of(g.gtree.root, g.gtree.root));
        }

        #[test]
        fn returns_false_when_ancestor_gnode_is_unoccupied() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            use crate::handle::GNodeId;
            let unoccupied = GNodeId::from_index(999);
            assert!(!g.is_ancestor_of(unoccupied, g.gtree.root));
        }
    }

    // ── Config accessors ─────────────────────────────────────────────────
    mod config_accessors {
        use super::*;

        #[test]
        fn depth_create_matches_config() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.depth_create(), 3);
        }

        #[test]
        fn depth_evict_matches_config() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.depth_evict(), 5);
        }

        #[test]
        fn depth_buffer_is_evict_minus_create() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.gtree.depth_buffer, 2);
        }

        #[test]
        fn headroom_is_three_to_the_power_of_depth_buffer_plus_one() {
            // depth_buffer = depth_evict(5) - depth_create(3) = 2
            // headroom = 3^(buffer+1) = 3^3 = 27
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.gtree.headroom, 27);
        }

        #[test]
        fn budget_is_none_when_not_configured() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(g.budget().is_none());
        }

        #[test]
        fn soft_limit_is_none_without_budget() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(g.gtree.soft_limit.is_none());
        }
    }

    // ── uniform_contour_depth_of ─────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod uniform_contour_depth_of_fn {
        use super::*;
        use crate::arena::Arena;
        use crate::handle::GNodeId;
        use crate::nodes::gnode::GNode;
        use crate::tree::gtree::uniform_contour_depth_of;

        fn make_terminal(lo: u8, hi: u8) -> GNode<u8, u32> {
            GNode::new_leaf(lo, hi, 0u32, None)
        }

        #[test]
        fn terminal_root_returns_some_depth() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            // Fresh graph root is Terminal
            let result = uniform_contour_depth_of(&g.gtree.nodes, g.gtree.root, 8);
            assert!(result.is_some());
        }

        #[test]
        fn semi_internal_root_returns_none() {
            let mut g = GvGraph::<u8, u32, 8>::new(make_config());
            // Manually give the root a single (fake) left child → SemiInternal
            let fake_child = GNodeId::from_index(999);
            g.gtree
                .nodes
                .get_mut(g.gtree.root.index())
                .link_left(fake_child);
            let result = uniform_contour_depth_of(&g.gtree.nodes, g.gtree.root, 8);
            assert_eq!(result, None);
            // Restore so subsequent arena operations are not corrupted
            g.gtree
                .nodes
                .get_mut(g.gtree.root.index())
                .clear_child(fake_child);
        }

        #[test]
        fn internal_root_with_equal_depth_children_returns_some() {
            // For u8 / N=8, domain_max=255 means bootstrap splits produce unequal widths.
            // Construct explicitly: root=[0,128) Internal, left=[0,64) and right=[64,128)
            // both Terminal with equal width 64 → equal depth (8 - floor(log2(64)) = 2).
            let mut gnodes: Arena<GNode<u8, u32>> = Arena::new();
            let left_id = GNodeId::from_index(gnodes.alloc(make_terminal(0, 64)));
            let right_id = GNodeId::from_index(gnodes.alloc(make_terminal(64, 128)));
            let mut root = GNode::new_leaf(0u8, 128u8, 0u32, None);
            root.link_left(left_id);
            root.link_right(right_id);
            let root_id = GNodeId::from_index(gnodes.alloc(root));
            let result = uniform_contour_depth_of(&gnodes, root_id, 8);
            assert_eq!(result, Some(2));
        }

        #[test]
        fn internal_root_with_unequal_depth_children_returns_none() {
            let mut g = GvGraph::<u8, u32, 8>::new(make_config());
            // First obs: bootstrap split with odd domain (u8/N=8) → unequal-width children
            // left=[0,127) width=127 depth=2, right=[127,255] width=128 depth=1 → mismatch → None
            g.observe(64u8, 3u32);
            let result = uniform_contour_depth_of(&g.gtree.nodes, g.gtree.root, 8);
            assert_eq!(result, None);
        }
    }
}
