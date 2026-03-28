//! Zero-cost no-op plateau tracker for builds without `dynamic-contour-tracking`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::arena::Arena;
use crate::handle::GNodeId;
use crate::nodes::gnode::{GNode, GState};
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::traits::{Accumulator, Coordinate, PlateauTracking};

/// Zero-cost placeholder that satisfies the [`PlateauTracking`] bound while
/// performing no work.  Selected when `dynamic-contour-tracking` is disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPlateauTracker;

impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V> for NoopPlateauTracker {
    #[inline(always)]
    fn on_observe(&mut self, _gnodes: &Arena<GNode<C, V>>, _g_id: GNodeId, _value: V) {}

    #[inline(always)]
    fn on_bootstrap_split(
        &mut self,
        _gnodes: &Arena<GNode<C, V>>,
        _g_id: GNodeId,
        _left_id: GNodeId,
    ) {
    }

    #[inline(always)]
    fn on_catalytic_split(
        &mut self,
        _gnodes: &Arena<GNode<C, V>>,
        _g_id: GNodeId,
        _left_id: GNodeId,
    ) {
    }

    #[inline(always)]
    fn on_evict(
        &mut self,
        _gnodes: &Arena<GNode<C, V>>,
        _gnode_id: GNodeId,
        _parent_id: GNodeId,
        _parent_state_after: GState,
        _parent_lo: C,
        _parent_hi: C,
    ) {
    }

    #[inline(always)]
    fn on_legacy_promotes_batched(
        &mut self,
        _gnodes: &Arena<GNode<C, V>>,
        _new_gnodes: &[GNodeId],
    ) {
    }

    #[inline(always)]
    fn normalize(&mut self, _gnodes: &Arena<GNode<C, V>>) {}

    #[inline(always)]
    fn repair_p_i4(&mut self, _gnodes: &Arena<GNode<C, V>>) {}

    #[inline(always)]
    fn recompute_sums(&mut self, _gnodes: &Arena<GNode<C, V>>, _label: &str) {}

    #[inline(always)]
    fn set_dirty(&mut self) {}

    #[inline(always)]
    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        Cow::Owned(BTreeMap::new())
    }
}
