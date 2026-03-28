use std::borrow::Cow;
use std::collections::BTreeMap;

use super::{Accumulator, Coordinate};
use crate::arena::Arena;
use crate::handle::GNodeId;
use crate::nodes::gnode::{GNode, GState};
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
pub trait PlateauTracking<C: Coordinate, V: Accumulator> {
    /// Called after an observation has been routed to `g_id` and the G-node's
    /// own value updated.  `value` is the already-accumulated contribution.
    fn on_observe(&mut self, gnodes: &Arena<GNode<C, V>>, g_id: GNodeId, value: V);

    /// Called after a G-node has been split via a bootstrap split.
    fn on_bootstrap_split(&mut self, gnodes: &Arena<GNode<C, V>>, g_id: GNodeId, left_id: GNodeId);

    /// Called after a G-node has been split via a catalytic split.
    fn on_catalytic_split(&mut self, gnodes: &Arena<GNode<C, V>>, g_id: GNodeId, left_id: GNodeId);

    /// Called after a G-node has been evicted from the tree.
    fn on_evict(
        &mut self,
        gnodes: &Arena<GNode<C, V>>,
        gnode_id: GNodeId,
        parent_id: GNodeId,
        parent_state_after: GState,
        parent_lo: C,
        parent_hi: C,
    );

    /// Called after a batch of legacy-promote operations.
    fn on_legacy_promotes_batched(&mut self, gnodes: &Arena<GNode<C, V>>, new_gnodes: &[GNodeId]);

    /// Runs the full plateau normalisation pass (consolidate + repair).
    fn normalize(&mut self, gnodes: &Arena<GNode<C, V>>);

    /// Restores the P-I4 invariant for all semi-internal G-nodes.
    fn repair_p_i4(&mut self, gnodes: &Arena<GNode<C, V>>);

    /// Recomputes the `sum` field for every tracked plateau from the current
    /// G-tree node sums.  Called after bulk weight mutations (e.g. `decay`).
    fn recompute_sums(&mut self, gnodes: &Arena<GNode<C, V>>, label: &str);

    /// Marks the tracker dirty so that the next `normalize` call will
    /// recompute the plateau map from scratch.
    fn set_dirty(&mut self);

    /// Read access to all currently tracked plateaus, keyed by basis edge.
    ///
    /// Returns `Cow::Owned(BTreeMap::new())` for no-op trackers, and
    /// `Cow::Borrowed(&self.plateaus)` for the dynamic tracker.  This
    /// matches the return type of the existing `GvGraph::plateaus()` method.
    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>>;

    /// Debug hook: verifies the plateau mirror against `fresh` (a map just
    /// rebuilt by walking the G-tree).
    ///
    /// Called by `GvGraph::debug_assert_plateau_mirror_consistency` at key
    /// algorithm checkpoints.  `NoopPlateauTracker` provides an empty default
    /// implementation; `DynamicPlateauTracker` compares the maps and panics on
    /// divergence.
    fn debug_assert_mirror_consistency(
        &self,
        gnodes: &Arena<GNode<C, V>>,
        fresh: &BTreeMap<BasisEdge<C>, Plateau<C, V>>,
        label: &str,
    ) {
        let _ = (gnodes, fresh, label);
    }
}
