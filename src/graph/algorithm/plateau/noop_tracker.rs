//! Zero-cost no-op plateau tracker for builds without `dynamic-contour-tracking`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::handle::GNodeId;
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::traits::{Accumulator, Coordinate, PlateauTracking};

/// Zero-cost placeholder that satisfies the [`PlateauTracking`] bound while
/// performing no work.  Selected when `dynamic-contour-tracking` is disabled.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct NoopPlateauTracker;

impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V> for NoopPlateauTracker {
    #[inline(always)]
    fn on_observe(&mut self, _id: GNodeId, _coord: C, _value: V) {}

    #[inline(always)]
    fn on_split(
        &mut self,
        _parent: GNodeId,
        _child_lo: GNodeId,
        _child_hi: GNodeId,
    ) {
    }

    #[inline(always)]
    fn on_evict(&mut self, _id: GNodeId) {}

    #[inline(always)]
    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        Cow::Owned(BTreeMap::new())
    }
}
