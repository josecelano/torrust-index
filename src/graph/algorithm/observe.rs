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
            crate::diagnostics::diagnostic::audit_violations(
                &self.vnodes,
                &self.violations,
                "POST-SPLIT",
            );
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
            let remaining = crate::diagnostics::diagnostic::audit_violations(
                &self.vnodes,
                &self.violations,
                "POST-OBSERVE",
            );
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

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph};

    type G = GvGraph<u8, u32, 8>;

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

    fn fresh_graph() -> G {
        GvGraph::new(make_config())
    }

    // ── observe ──────────────────────────────────────────────────────────
    mod observe {
        use super::*;

        #[test]
        fn single_observation_increases_total_sum() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            assert_eq!(g.total_sum(), 10u32);
        }

        #[test]
        fn multiple_observations_accumulate() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            g.observe(128u8, 20u32);
            assert_eq!(g.total_sum(), 30u32);
        }

        #[test]
        fn repeated_observations_at_same_coord_accumulate() {
            let mut g = fresh_graph();
            for _ in 0..3 {
                g.observe(0u8, 5u32);
            }
            assert_eq!(g.total_sum(), 15u32);
        }

        #[test]
        fn enough_observations_trigger_split() {
            // split_threshold = 2, so 3 observations at same coord should split
            let mut g = fresh_graph();
            let initial_nodes = g.node_count();
            for _ in 0..3 {
                g.observe(64u8, 10u32);
            }
            // After a split the node_count should have grown
            assert!(g.node_count() > initial_nodes);
        }

        #[test]
        fn observation_does_not_violate_invariants() {
            let mut g = fresh_graph();
            g.observe(0u8, 1u32);
            g.observe(128u8, 1u32);
            g.observe(64u8, 1u32);
            // No panic = invariants respected
        }

        #[test]
        fn observe_with_bounded_eviction_does_not_panic() {
            // bounded_eviction=true exercises the check_evictions_bounded path.
            // budget=28: depth_buffer=2, headroom=3^3=27, soft_limit=28-27=1.
            // After first split node_count=3 > soft_limit=1 → check_evictions_bounded
            // is called, but nodes are at depth 1-2 << depth_evict=4 → 0 evictions.
            let cfg = Config {
                split_threshold: 2,
                depth_create: 2,
                depth_evict: 4,
                budget: Some(28),
                alpha_relax: 0.5,
                bounded_eviction: true,
            };
            let mut g: G = GvGraph::new(cfg);
            g.observe(64u8, 3u32); // bootstrap split → node_count=3 > soft_limit=1
            g.observe(32u8, 3u32);
        }

        #[test]
        fn observe_with_budget_and_unbounded_eviction() {
            // check_evictions (unbounded) path when node_count > soft_limit.
            let cfg = Config {
                split_threshold: 2,
                depth_create: 2,
                depth_evict: 4,
                budget: Some(28),
                alpha_relax: 0.5,
                bounded_eviction: false,
            };
            let mut g: G = GvGraph::new(cfg);
            g.observe(64u8, 3u32); // bootstrap split → node_count=3 > soft_limit=1
            g.observe(32u8, 3u32);
        }
    }
}
