#[cfg(feature = "dynamic-contour-tracking")]
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;

use crate::arena::Arena;
use crate::handle::{GNodeId, VNodeId};
use crate::nodes::gnode::{GNode, GNodeChildren};
use crate::nodes::vnode::VNode;
use crate::spatial::node::Node;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau::{BasisEdge, Plateau};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau_basis::PlateauBasis;
use crate::traits::{Accumulator, Coordinate};

use super::config::Config;

#[allow(dead_code)]
pub fn uniform_contour_depth_of<C: Coordinate, V: Accumulator>(
    gnodes: &Arena<GNode<C, V>>,
    gid: GNodeId,
    n: u32,
) -> Option<u32> {
    use crate::nodes::gnode::GState;
    use crate::tree::gtree::gnode_depth_from_interval;
    let g = gnodes.get(gid.index());
    match g.state() {
        GState::Terminal => Some(gnode_depth_from_interval(g.lo, g.hi, n)),
        GState::SemiInternal => None,
        GState::Internal => {
            let ld = g
                .left
                .and_then(|l| uniform_contour_depth_of(gnodes, l, n))?;
            let rd = g
                .right
                .and_then(|r| uniform_contour_depth_of(gnodes, r, n))?;
            if ld == rd { Some(ld) } else { None }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GvGraph<C: Coordinate, V: Accumulator, const N: u32> {
    pub(crate) gnodes: Arena<GNode<C, V>>,

    pub(crate) vnodes: Arena<VNode<V>>,

    pub(crate) g_root: GNodeId,

    pub(crate) v_root: Option<VNodeId>,

    pub(crate) config: Config<V>,

    pub(crate) violations: Vec<VNodeId>,

    pub(crate) node_count: u32,

    pub(crate) terminal_count: u32,

    pub(crate) live_depth_evict: u32,

    pub(crate) live_depth_create: u32,

    pub(crate) depth_buffer: u32,

    pub(crate) headroom: usize,

    pub(crate) soft_limit: Option<usize>,

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateaus: BTreeMap<BasisEdge<C>, Plateau<C, V>>,

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) pending_p_i4: Vec<(GNodeId, BasisEdge<C>)>,

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateau_basis: PlateauBasis<C>,

    #[cfg(feature = "dynamic-contour-tracking")]
    pub(crate) plateaus_dirty: bool,
}

impl<C: Coordinate, V: Accumulator, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    pub fn new(config: Config<V>) -> Self {
        const { assert!(N <= C::BITS, "N must be <= C::BITS") };

        config.validate();

        let mut gnodes = Arena::new();
        let mut vnodes = Arena::new();

        let root_gnode = GNode {
            lo: C::zero(),
            hi: C::domain_max(N),
            sum: V::zero(),
            own: V::zero(),
            left: None,
            right: None,
            parent: None,
            entry: None,
        };
        let g_root = GNodeId::from_index(gnodes.alloc(root_gnode));

        let root_entry = VNode {
            intensity: V::zero(),
            parent: None,
            cached_depth: AtomicU32::new(0),
            kind: crate::nodes::vnode::VKind::Entry {
                gnode: g_root,
                is_exposed: true,
                is_evictable: true,
            },
        };
        let v_root_id = VNodeId::from_index(vnodes.alloc(root_entry));
        gnodes.get_mut(g_root.index()).entry = Some(v_root_id);

        let live_depth_evict = config.depth_evict;
        let live_depth_create = config.depth_create;
        let depth_buffer = config.depth_evict - config.depth_create;
        let headroom = 3usize.pow(depth_buffer + 1);
        let convergence_bound = 2 * (live_depth_create as usize).saturating_sub(1);
        let required_headroom = headroom.max(convergence_bound);
        let soft_limit = config.budget.map(|b| {
            let s = b - required_headroom;
            assert!(
                s >= 1,
                "soft_limit must be >= 1 (budget={b}, headroom={required_headroom})"
            );
            s
        });

        #[cfg(feature = "dynamic-contour-tracking")]
        let (plateaus, plateau_basis) = {
            use crate::tree::gtree::gnode_depth_from_interval;
            let root_key = BasisEdge(C::zero());
            let root_depth = gnode_depth_from_interval(C::zero(), C::domain_max(N), N);
            let mut pb = PlateauBasis::new();
            pb.insert(root_key, g_root);
            let mut map = BTreeMap::new();
            map.insert(
                root_key,
                Plateau {
                    basis_edge: root_key,
                    start: C::zero(),
                    end: C::domain_max(N),
                    depth: root_depth,
                    sum: V::zero(),
                },
            );
            (map, pb)
        };

        Self {
            gnodes,
            vnodes,
            g_root,
            v_root: Some(v_root_id),
            config,
            violations: Vec::new(),
            node_count: 1,
            terminal_count: 1,
            live_depth_evict,
            live_depth_create,
            depth_buffer,
            headroom,
            soft_limit,
            #[cfg(feature = "dynamic-contour-tracking")]
            plateaus,
            #[cfg(feature = "dynamic-contour-tracking")]
            pending_p_i4: Vec::new(),
            #[cfg(feature = "dynamic-contour-tracking")]
            plateau_basis,
            #[cfg(feature = "dynamic-contour-tracking")]
            plateaus_dirty: false,
        }
    }

    #[must_use]
    #[inline]
    pub const fn node_count(&self) -> u32 {
        self.node_count
    }

    #[must_use]
    #[inline]
    pub const fn terminal_count(&self) -> u32 {
        self.terminal_count
    }

    #[must_use]
    #[inline]
    pub const fn config(&self) -> &Config<V> {
        &self.config
    }

    #[must_use]
    #[inline]
    pub const fn budget(&self) -> Option<usize> {
        self.config.budget
    }

    #[must_use]
    #[inline]
    pub const fn g_root(&self) -> GNodeId {
        self.g_root
    }

    #[must_use]
    #[inline]
    pub(crate) const fn v_root(&self) -> Option<VNodeId> {
        self.v_root
    }

    #[must_use]
    #[inline]
    pub fn total_sum(&self) -> V {
        self.gnodes.get(self.g_root.index()).sum
    }

    #[must_use]
    #[inline]
    pub(crate) const fn gnodes(&self) -> &Arena<GNode<C, V>> {
        &self.gnodes
    }

    #[must_use]
    #[inline]
    pub(crate) const fn vnodes(&self) -> &Arena<VNode<V>> {
        &self.vnodes
    }

    #[cfg(test)]
    #[must_use]
    #[inline]
    pub(crate) fn has_pending_violations(&self) -> bool {
        !self.violations.is_empty()
    }

    #[must_use]
    #[inline]
    pub const fn depth_evict(&self) -> u32 {
        self.live_depth_evict
    }

    #[must_use]
    #[inline]
    pub const fn depth_create(&self) -> u32 {
        self.live_depth_create
    }

    #[must_use]
    #[inline]
    pub const fn depth_buffer(&self) -> u32 {
        self.depth_buffer
    }

    #[must_use]
    #[inline]
    pub const fn headroom(&self) -> usize {
        self.headroom
    }

    #[allow(clippy::doc_markdown)]
    #[must_use]
    #[inline]
    pub const fn soft_limit(&self) -> Option<usize> {
        self.soft_limit
    }

    #[must_use]
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn gnode_depth(&self, gid: GNodeId) -> u32 {
        let g = self.gnodes.get(gid.index());
        crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N)
    }

    #[must_use]
    pub fn gnode_info(&self, id: GNodeId) -> Option<Node<C, V>> {
        if !self.gnodes.is_occupied(id.index()) {
            return None;
        }
        let g = self.gnodes.get(id.index());
        Some(Node {
            start: g.lo,
            end: g.hi,
            own: g.own,
            sum: g.sum,
            depth: crate::tree::gtree::gnode_depth_from_interval(g.lo, g.hi, N),
            state: g.state(),
            gnode_id: id,
            parent: g.parent,
        })
    }

    #[doc(hidden)]
    #[must_use]
    pub fn gnode_children(&self, id: GNodeId) -> Option<GNodeChildren> {
        if !self.gnodes.is_occupied(id.index()) {
            return None;
        }
        let g = self.gnodes.get(id.index());
        Some(GNodeChildren {
            left: g.left,
            right: g.right,
        })
    }

    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: GNodeId, descendant: GNodeId) -> bool {
        if !self.gnodes.is_occupied(ancestor.index())
            || !self.gnodes.is_occupied(descendant.index())
        {
            return false;
        }
        let a = self.gnodes.get(ancestor.index());
        let d = self.gnodes.get(descendant.index());
        a.lo <= d.lo && a.hi >= d.hi && (a.lo != d.lo || a.hi != d.hi)
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph};

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            depth_create: 3,
            depth_evict: 5,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        }
    }

    // ── GvGraph::new ─────────────────────────────────────────────────────
    mod new {
        use super::*;

        #[test]
        fn starts_with_one_node() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.node_count(), 1);
        }

        #[test]
        fn starts_with_one_terminal() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.terminal_count(), 1);
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
            assert!(g.gnode_info(g.g_root()).is_some());
        }

        #[test]
        fn root_covers_full_domain() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.g_root()).unwrap();
            use crate::traits::Coordinate;
            assert_eq!(info.start, u8::zero());
            assert_eq!(info.end, u8::domain_max(8));
        }

        #[test]
        fn root_is_terminal_at_start() {
            use crate::nodes::gnode::GState;
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.g_root()).unwrap();
            assert_eq!(info.state, GState::Terminal);
        }

        #[test]
        fn root_is_root_node() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            let info = g.gnode_info(g.g_root()).unwrap();
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
            let ch = g.gnode_children(g.g_root()).unwrap();
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
            assert!(!g.is_ancestor_of(g.g_root(), g.g_root()));
        }

        #[test]
        fn returns_false_when_ancestor_gnode_is_unoccupied() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            use crate::handle::GNodeId;
            let unoccupied = GNodeId::from_index(999);
            assert!(!g.is_ancestor_of(unoccupied, g.g_root()));
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
            assert_eq!(g.depth_buffer(), 2);
        }

        #[test]
        fn headroom_is_three_to_the_power_of_depth_buffer_plus_one() {
            // depth_buffer = depth_evict(5) - depth_create(3) = 2
            // headroom = 3^(buffer+1) = 3^3 = 27
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert_eq!(g.headroom(), 27);
        }

        #[test]
        fn budget_is_none_when_not_configured() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(g.budget().is_none());
        }

        #[test]
        fn soft_limit_is_none_without_budget() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            assert!(g.soft_limit().is_none());
        }
    }

    // ── uniform_contour_depth_of ─────────────────────────────────────────
    mod uniform_contour_depth_of_fn {
        use super::*;
        use crate::arena::Arena;
        use crate::graph::uniform_contour_depth_of;
        use crate::handle::GNodeId;
        use crate::nodes::gnode::GNode;

        fn make_terminal(lo: u8, hi: u8) -> GNode<u8, u32> {
            GNode {
                lo,
                hi,
                sum: 0,
                own: 0,
                left: None,
                right: None,
                parent: None,
                entry: None,
            }
        }

        #[test]
        fn terminal_root_returns_some_depth() {
            let g = GvGraph::<u8, u32, 8>::new(make_config());
            // Fresh graph root is Terminal
            let result = uniform_contour_depth_of(&g.gnodes, g.g_root, 8);
            assert!(result.is_some());
        }

        #[test]
        fn semi_internal_root_returns_none() {
            let mut g = GvGraph::<u8, u32, 8>::new(make_config());
            // Manually give the root a single (fake) left child → SemiInternal
            let fake_child = GNodeId::from_index(999);
            g.gnodes.get_mut(g.g_root.index()).left = Some(fake_child);
            let result = uniform_contour_depth_of(&g.gnodes, g.g_root, 8);
            assert_eq!(result, None);
            // Restore so subsequent arena operations are not corrupted
            g.gnodes.get_mut(g.g_root.index()).left = None;
        }

        #[test]
        fn internal_root_with_equal_depth_children_returns_some() {
            // For u8 / N=8, domain_max=255 means bootstrap splits produce unequal widths.
            // Construct explicitly: root=[0,128) Internal, left=[0,64) and right=[64,128)
            // both Terminal with equal width 64 → equal depth (8 - floor(log2(64)) = 2).
            let mut gnodes: Arena<GNode<u8, u32>> = Arena::new();
            let left_id = GNodeId::from_index(gnodes.alloc(make_terminal(0, 64)));
            let right_id = GNodeId::from_index(gnodes.alloc(make_terminal(64, 128)));
            let root_id = GNodeId::from_index(gnodes.alloc(GNode {
                lo: 0,
                hi: 128,
                sum: 0,
                own: 0,
                left: Some(left_id),
                right: Some(right_id),
                parent: None,
                entry: None,
            }));
            let result = uniform_contour_depth_of(&gnodes, root_id, 8);
            assert_eq!(result, Some(2));
        }

        #[test]
        fn internal_root_with_unequal_depth_children_returns_none() {
            let mut g = GvGraph::<u8, u32, 8>::new(make_config());
            // First obs: bootstrap split with odd domain (u8/N=8) → unequal-width children
            // left=[0,127) width=127 depth=2, right=[127,255] width=128 depth=1 → mismatch → None
            g.observe(64u8, 3u32);
            let result = uniform_contour_depth_of(&g.gnodes, g.g_root, 8);
            assert_eq!(result, None);
        }
    }
}
