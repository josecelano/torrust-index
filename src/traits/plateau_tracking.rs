use std::borrow::Cow;
use std::collections::BTreeMap;

use super::{Accumulator, Coordinate};
use crate::handle::GNodeId;
use crate::spatial::plateau::{BasisEdge, Plateau};

/// Strategy trait that encapsulates all plateau-state updates.
///
/// `GvGraph` delegates every plateau mutation to the concrete tracker it was
/// constructed with:
///
/// - [`DynamicPlateauTracker`] — maintains the plateau mirror incrementally
///   (used when the `dynamic-contour-tracking` feature is enabled).
/// - [`NoopPlateauTracker`] — zero-cost empty implementation for builds where
///   dynamic tracking is disabled.
///
/// This replaces the `#[cfg(feature = "dynamic-contour-tracking")]` guards that
/// were previously scattered through every algorithm file.
pub(crate) trait PlateauTracking<C: Coordinate, V: Accumulator> {
    /// Called after an observation has been routed to `id` and the G-node's
    /// own value updated.
    fn on_observe(&mut self, id: GNodeId, coord: C, value: V);

    /// Called after a G-node has been split into `child_lo` and `child_hi`
    /// (both bootstrap and catalytic splits).
    fn on_split(&mut self, parent: GNodeId, child_lo: GNodeId, child_hi: GNodeId);

    /// Called after a G-node has been evicted from the tree.
    fn on_evict(&mut self, id: GNodeId);

    /// Read access to all currently tracked plateaus, keyed by basis edge.
    ///
    /// Returns `Cow::Owned(BTreeMap::new())` for no-op trackers, and
    /// `Cow::Borrowed(&self.plateaus)` for the dynamic tracker.  This
    /// matches the return type of the existing `GvGraph::plateaus()` method.
    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>>;
}
