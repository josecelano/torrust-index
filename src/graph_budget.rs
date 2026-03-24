use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::{evict, rebalance, vtree};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {

    pub(crate) fn handle_legacy_promotes(&mut self, new_gnodes: &[GNodeId]) {
        for &_new_gid in new_gnodes {
            self.node_count += 1;

            self.terminal_count += 1;
        }
        self.plateau_after_legacy_promotes_batched(new_gnodes);
    }

    pub(crate) fn adjust_depth_gates(&mut self) {
        let Some(budget) = self.config.budget else {
            return;
        };
        let count = self.node_count as usize;

        let convergence_bound = 2 * (self.live_depth_create as usize).saturating_sub(1);
        let required_headroom = self.headroom.max(convergence_bound);
        let soft_limit = budget - required_headroom;
        assert!(
            soft_limit >= 1,
            "soft_limit must be >= 1 (budget={budget}, headroom={required_headroom}, \
             D_c={}, buffer={})",
            self.live_depth_create,
            self.depth_buffer
        );
        self.soft_limit = Some(soft_limit);

        if count > soft_limit {

            let floor = self.depth_buffer + 1;
            if self.live_depth_evict > floor {
                self.live_depth_evict -= 1;
                self.live_depth_create = self.live_depth_evict - self.depth_buffer;
                tracing::debug!(
                    new_d_evict = self.live_depth_evict,
                    new_d_create = self.live_depth_create,
                    count,
                    soft_limit,
                    "depth gates tightened",
                );
            }
        } else {
            #[allow(clippy::cast_precision_loss)]
            let threshold = soft_limit as f64 * self.config.alpha_relax;
            #[allow(clippy::cast_precision_loss)]
            let count_f = count as f64;
            if count_f < threshold {

                self.live_depth_evict += 1;
                self.live_depth_create = self.live_depth_evict - self.depth_buffer;
                tracing::debug!(
                    new_d_evict = self.live_depth_evict,
                    new_d_create = self.live_depth_create,
                    count,
                    soft_limit,
                    "depth gates relaxed",
                );
            }
        }
    }

    pub fn check_evictions(&mut self) -> u32 {
        let _span = tracing::debug_span!("check_evictions", d_evict = self.live_depth_evict).entered();
        self.evict_candidates(None)
    }

    pub(crate) fn check_evictions_bounded(&mut self, stop_at: usize) -> u32 {
        self.evict_candidates(Some(stop_at))
    }

    #[allow(clippy::too_many_lines)]
    fn evict_candidates(&mut self, stop_at: Option<usize>) -> u32 {
        let candidates = evict::scan_for_candidates(self);
        let _span = tracing::debug_span!("evict_batch", candidate_count = candidates.len(),).entered();
        let mut evicted: u32 = 0;

        for v_id in candidates {

            if let Some(limit) = stop_at {
                if evicted as usize >= limit {
                    break;
                }
            }

            if !self.vnodes.is_occupied(v_id.index()) {
                continue;
            }
            match &self.vnodes.get(v_id.index()).kind {
                crate::vnode::VKind::Entry { gnode, is_evictable, .. } => {
                    if !is_evictable {
                        continue;
                    }
                    if *gnode == self.g_root {
                        continue;
                    }

                    let depth = vtree::v_depth(&self.vnodes, v_id);
                    if depth <= self.live_depth_evict {
                        continue;
                    }
                }
                crate::vnode::VKind::Structural { .. } => continue,
            }

            evict::evict_tip(self, v_id);
            evicted += 1;

            if tracing::enabled!(tracing::Level::DEBUG) {
                crate::diagnostic::audit_violations(&self.vnodes, &self.violations, "POST-EVICT");
            }

            #[cfg(feature = "dynamic-contour-tracking")]
            if cfg!(debug_assertions) {
                self.debug_assert_plateau_mirror_consistency(&format!("POST-EVICT-SINGLE-{evicted}"));
            }
        }

        if evicted > 0 {
            let new_gnodes = rebalance::rebalance(
                &mut self.vnodes,
                &mut self.gnodes,
                &mut self.violations,
                self.live_depth_evict,
            );
            if !new_gnodes.is_empty() {
                self.handle_legacy_promotes(&new_gnodes);
            }

            self.repair_p_i4();

            #[cfg(feature = "dynamic-contour-tracking")]
            {
                self.plateaus_dirty = true;
            }
            self.normalize_plateaus();

            self.repair_p_i4();
        }

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-EVICT");
        }

        evicted
    }
}
