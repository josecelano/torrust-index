use super::{Accumulator, Coordinate};
use crate::handle::GNodeId;
use crate::spatial::plateau::Plateau;

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

    /// Read-only slice of all currently tracked plateaus.
    fn plateaus(&self) -> &[Plateau<C, V>];
}
