//! Incremental plateau-mirror tracker for `dynamic-contour-tracking` builds.
//!
//! [`DynamicPlateauTracker`] owns all plateau-related state that was previously
//! scattered across `GvGraph` as `#[cfg(feature = "dynamic-contour-tracking")]`
//! fields.  It implements the [`PlateauTracking`] strategy trait and will be
//! threaded into `GvGraph` as a type parameter in Phase 7 Step 7.3.

#![cfg(feature = "dynamic-contour-tracking")]

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::handle::GNodeId;
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::spatial::plateau_basis::PlateauBasis;
use crate::traits::{Accumulator, Coordinate, PlateauTracking};

/// Incrementally maintains a mirror of the plateau structure as observations,
/// splits, and evictions mutate the G-tree.
///
/// All heavyweight plateau logic currently lives on `GvGraph` as `impl` blocks
/// in `plateau/mod.rs` and `plateau/normalise.rs`.  This struct holds the
/// *state* for those algorithms; the algorithms themselves will be migrated here
/// in Step 7.3.
pub(crate) struct DynamicPlateauTracker<C: Coordinate, V: Accumulator> {
    /// The plateau mirror: maps each basis edge to its current plateau.
    pub(super) plateaus: BTreeMap<BasisEdge<C>, Plateau<C, V>>,

    /// Pending (semi-internal node, plateau key) pairs that must be checked by
    /// the P-I4 repair pass.
    #[allow(dead_code)]
    pub(super) pending_p_i4: Vec<(GNodeId, BasisEdge<C>)>,

    /// Reverse index: maps each G-node id to the basis-edge key of the plateau
    /// it contributes to.
    #[allow(dead_code)]
    pub(super) plateau_basis: PlateauBasis<C>,

    /// True when at least one mutating operation has occurred since the last
    /// `normalize_plateaus` call.
    pub(super) plateaus_dirty: bool,

    /// The coordinate bit-width (`N` from `GvGraph<C, V, N>`), stored at
    /// runtime so depth computations do not require the const generic here.
    #[allow(dead_code)]
    pub(super) n_bits: u32,
}

impl<C: Coordinate, V: Accumulator> DynamicPlateauTracker<C, V> {
    /// Creates a fresh tracker configured for a tree with coordinate range
    /// `n_bits` wide.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn new(n_bits: u32) -> Self {
        Self {
            plateaus: BTreeMap::new(),
            pending_p_i4: Vec::new(),
            plateau_basis: PlateauBasis::new(),
            plateaus_dirty: false,
            n_bits,
        }
    }
}

// ── PlateauTracking impl ─────────────────────────────────────────────────────
//
// The three trait methods below are intentional stubs.  The full plateau logic
// still lives on `GvGraph` as `impl` blocks (see `plateau/mod.rs` and
// `plateau/normalise.rs`).  Step 7.3 will move that logic here and wire the
// tracker into `GvGraph` by replacing `#[cfg]`-guarded inline code with
// `self.tracker.on_*(…)` calls.

impl<C: Coordinate, V: Accumulator> PlateauTracking<C, V> for DynamicPlateauTracker<C, V> {
    #[inline]
    fn on_observe(&mut self, _id: GNodeId, _coord: C, _value: V) {
        // TODO(Phase 7 Step 7.3): migrate plateau_after_observe logic here.
        self.plateaus_dirty = true;
    }

    #[inline]
    fn on_split(&mut self, _parent: GNodeId, _child_lo: GNodeId, _child_hi: GNodeId) {
        // TODO(Phase 7 Step 7.3): migrate plateau_after_{bootstrap,catalytic}_split here.
        self.plateaus_dirty = true;
    }

    #[inline]
    fn on_evict(&mut self, _id: GNodeId) {
        // TODO(Phase 7 Step 7.3): migrate plateau_after_evict logic here.
        self.plateaus_dirty = true;
    }

    fn plateaus(&self) -> Cow<'_, BTreeMap<BasisEdge<C>, Plateau<C, V>>> {
        Cow::Borrowed(&self.plateaus)
    }
}
