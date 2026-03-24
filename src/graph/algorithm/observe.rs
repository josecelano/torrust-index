use crate::graph::GvGraph;
use crate::graph::algorithm::{rebalance, split};
use crate::traits::{Accumulator, Coordinate, Inspectable, Observation};
use crate::tree::{gtree, vtree};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    pub fn observe<O: Observation<V>>(&mut self, coord: C, delta: O) {
        let g_id = gtree::route_to_receiver(&self.gnodes, self.g_root, coord);
        let _span = tracing::debug_span!("observe", g = g_id.index()).entered();

        let g = self.gnodes.get_mut(g_id.index());
        g.own = O::accumulate(g.own, delta);

        debug_assert!(
            g.own >= V::zero(),
            "observe: P2 violation — accumulated own value {:?} < zero after delta; \
             negative accumulations are not supported (ADR-M-033)",
            g.own,
        );

        if let Some(entry_id) = self.gnodes.get(g_id.index()).entry {
            let v = self.vnodes.get_mut(entry_id.index());
            v.intensity = O::accumulate(v.intensity, delta);
            let new_intensity = v.intensity;

            vtree::update_parent_cached_intensity(&mut self.vnodes, entry_id, new_intensity);
            vtree::propagate_v_sums(&mut self.vnodes, entry_id);

            let mut check_id = Some(entry_id);
            while let Some(id) = check_id {
                if rebalance::is_violated(&self.vnodes, id) {
                    tracing::debug!(
                        violated = %rebalance::Nd(&self.vnodes, id),
                        entry = entry_id.index(),
                        "enqueuing violated ancestor",
                    );
                    self.violations.push(id);
                }
                check_id = self.vnodes.get(id.index()).parent;
            }
        }

        gtree::recompute_g_sums(&mut self.gnodes, g_id);

        self.plateau_after_observe::<O>(g_id, delta);

        split::attempt_split(self, g_id);

        if tracing::enabled!(tracing::Level::DEBUG) {
            crate::diagnostic::audit_violations(&self.vnodes, &self.violations, "POST-SPLIT");
        }

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-SPLIT");
        }

        let new_gnodes = rebalance::rebalance(
            &mut self.vnodes,
            &mut self.gnodes,
            &mut self.violations,
            self.live_depth_evict,
        );
        if !new_gnodes.is_empty() {
            self.handle_legacy_promotes(&new_gnodes);
            self.repair_p_i4();
        }

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-REBALANCE");
        }

        self.adjust_depth_gates();

        if let Some(soft_limit) = self.soft_limit {
            if self.node_count as usize > soft_limit {
                if self.config.bounded_eviction {
                    self.check_evictions_bounded(self.node_count as usize - soft_limit);
                } else {
                    self.check_evictions();
                }
            }
        }

        self.normalize_plateaus();

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_check_plateau_sums("POST-NORMALIZE");
        }

        self.repair_p_i4();

        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            let remaining =
                crate::diagnostic::audit_violations(&self.vnodes, &self.violations, "POST-OBSERVE");
            debug_assert!(
                remaining.is_empty(),
                "POST-OBSERVE: residual violations: {remaining:?}"
            );
        }

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-OBSERVE");
        }
    }
}
