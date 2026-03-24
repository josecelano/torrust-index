#[cfg(feature = "dynamic-contour-tracking")]
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;

use crate::arena::Arena;
use crate::nodes::gnode::GNode;
use crate::handle::{GNodeId, VNodeId};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::plateau::{BasisEdge, Plateau, PlateauBasis};
use crate::traits::{Accumulator, Coordinate};
use crate::view::Node;
use crate::nodes::vnode::VNode;

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GNodeChildren {

    pub left: Option<GNodeId>,

    pub right: Option<GNodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config<V: Accumulator> {

    pub split_threshold: V,

    pub depth_create: u32,

    pub depth_evict: u32,

    pub budget: Option<usize>,

    pub alpha_relax: f64,

    pub bounded_eviction: bool,
}

impl<V: Accumulator> Config<V> {

    fn validate(&self) {
        assert!(
            self.depth_create < self.depth_evict,
            "Config: D_create ({}) must be < D_evict ({}) (idea.md D-I3)",
            self.depth_create,
            self.depth_evict
        );
        assert!(
            self.depth_create >= 1,
            "Config: D_create ({}) must be >= 1",
            self.depth_create
        );
        assert!(
            self.alpha_relax > 0.0 && self.alpha_relax < 1.0,
            "Config: alpha_relax ({}) must be in (0.0, 1.0)",
            self.alpha_relax
        );
        if let Some(budget) = self.budget {
            let buffer = self.depth_evict - self.depth_create;
            let headroom = 3usize.pow(buffer + 1);
            let convergence = 2 * (self.depth_create as usize).saturating_sub(1);
            let required = headroom.max(convergence);
            assert!(
                budget > required,
                "Config: budget ({budget}) must be > max(3^(buffer+1), 2*(D_c-1)) \
                 = {required} (ADR-M-018: budget must exceed required headroom \
                 for hard ceiling guarantee)"
            );
        }
    }
}

#[allow(dead_code)]
pub fn uniform_contour_depth_of<C: Coordinate, V: Accumulator>(
    gnodes: &Arena<GNode<C, V>>,
    gid: GNodeId,
    n: u32,
) -> Option<u32> {
    use crate::nodes::gnode::GState;
    use crate::gtree::gnode_depth_from_interval;
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
            use crate::gtree::gnode_depth_from_interval;
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
    #[expect(dead_code)]
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
        crate::gtree::gnode_depth_from_interval(g.lo, g.hi, N)
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
            depth: crate::gtree::gnode_depth_from_interval(g.lo, g.hi, N),
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

