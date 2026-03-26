//! No-op stubs for all plateau methods when the `dynamic-contour-tracking`
//! feature is disabled.  Every method here is an empty `#[inline(always)]`
//! body so the rest of the codebase can call the same API unconditionally.
#![cfg(not(feature = "dynamic-contour-tracking"))]

use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    // --- from mod.rs ---

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn repair_p_i4(&mut self) {}

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_after_observe<O: crate::traits::Observation<V>>(
        &mut self,
        _g_id: GNodeId,
        _delta: O,
    ) {
    }

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_after_bootstrap_split(&mut self, _g_id: GNodeId, _left_id: GNodeId) {}

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_after_catalytic_split(&mut self, _g_id: GNodeId, _left_id: GNodeId) {}

    #[inline(always)]
    #[allow(dead_code, clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_after_legacy_promote(&mut self, _new_gid: GNodeId) {}

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_after_legacy_promotes_batched(&mut self, _new_gnodes: &[GNodeId]) {}

    #[inline(always)]
    #[allow(clippy::unused_self)]
    pub(crate) fn plateau_after_evict(
        &mut self,
        _gnode_id: GNodeId,
        _parent_id: GNodeId,
        _parent_state_after: crate::nodes::gnode::GState,
        _parent_lo: C,
        _parent_hi: C,
    ) {
    }

    // --- from normalise.rs ---

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn normalize_plateaus(&mut self) {}

    #[inline(always)]
    #[allow(dead_code, clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn consolidate_all_basis(&mut self) {}

    #[inline(always)]
    #[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub(crate) fn plateau_recompute_sums(&mut self, _label: &str) {}
}
