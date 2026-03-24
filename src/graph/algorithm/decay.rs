use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::traits::{Accumulator, Attenuatable, Coordinate, Inspectable};
use crate::graph::algorithm::rebalance;
use crate::tree::{gtree, vtree};

impl<C: Coordinate, V: Accumulator + Attenuatable + Inspectable, const N: u32> GvGraph<C, V, N> {
    pub fn decay(&mut self, root: GNodeId, attenuation: f64, q: f64) {
        let _span = tracing::debug_span!("decay", root = root.index(), attenuation, q,).entered();

        assert!(
            self.gnodes.is_occupied(root.index()),
            "decay: root handle (index {}) does not refer to a live G-node",
            root.index()
        );
        assert!(
            attenuation >= 0.0 && !attenuation.is_nan(),
            "decay: attenuation must be >= 0 and not NaN, got {attenuation}"
        );
        assert!(
            (0.0..=1.0).contains(&q) && !q.is_nan(),
            "decay: q must be in [0.0, 1.0] and not NaN, got {q}"
        );

        #[allow(clippy::float_cmp)]
        if attenuation == 1.0 {
            return;
        }

        let is_global = root == self.g_root;

        let _span = tracing::debug_span!("decay", root = root.index(), attenuation, q,).entered();

        if q == 0.0 {
            self.decay_uniform(root, attenuation, is_global);
        } else {
            self.decay_selective(root, attenuation, q, is_global);
        }
    }

    fn decay_uniform(&mut self, root: GNodeId, att: f64, is_global: bool) {
        let _span =
            tracing::debug_span!("decay_uniform", root = root.index(), att, is_global,).entered();

        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(gid) = stack.pop() {
            order.push(gid);

            let g = self.gnodes.get_mut(gid.index());
            g.own = g.own.attenuate(att);

            if let Some(v_id) = g.entry {
                let own = g.own;
                self.vnodes.get_mut(v_id.index()).intensity = own;
            }

            let g = self.gnodes.get(gid.index());
            if let Some(left) = g.left {
                stack.push(left);
            }
            if let Some(right) = g.right {
                stack.push(right);
            }
        }

        gtree::recompute_g_sums_subtree(&mut self.gnodes, &order);

        if !is_global {
            if let Some(parent) = self.gnodes.get(root.index()).parent {
                gtree::recompute_g_sums(&mut self.gnodes, parent);
            }
        }

        if let Some(v_root) = self.v_root {
            vtree::recompute_all_v_intensities(&mut self.vnodes, v_root);
        }

        self.violations = rebalance::find_violated_nodes(&self.vnodes);
        let new_gnodes = rebalance::rebalance(
            &mut self.vnodes,
            &mut self.gnodes,
            &mut self.violations,
            self.live_depth_evict,
        );
        if !new_gnodes.is_empty() {
            self.handle_legacy_promotes(&new_gnodes);
        }

        self.plateau_recompute_sums("DECAY-UNIFORM");

        #[cfg(feature = "dynamic-contour-tracking")]
        {
            self.plateaus_dirty = true;
        }
        self.normalize_plateaus();

        self.repair_p_i4();

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-DECAY-UNIFORM");
        }
    }

    #[allow(clippy::too_many_lines)]
    fn decay_selective(&mut self, root: GNodeId, att: f64, q: f64, is_global: bool) {
        let _span =
            tracing::debug_span!("decay_selective", root = root.index(), att, q, is_global,)
                .entered();

        let d_root = self.gnode_depth(root);

        let mut order = Vec::new();
        let mut max_depth = d_root;
        {
            let mut stack = vec![root];
            while let Some(gid) = stack.pop() {
                order.push(gid);
                let depth = self.gnode_depth(gid);
                if depth > max_depth {
                    max_depth = depth;
                }
                let g = self.gnodes.get(gid.index());
                if let Some(left) = g.left {
                    stack.push(left);
                }
                if let Some(right) = g.right {
                    stack.push(right);
                }
            }
        }
        let depth_range = max_depth - d_root;

        #[allow(clippy::float_cmp)]
        let factors: Vec<f64> = if att == 0.0 {
            (0..=depth_range)
                .map(|d_local| {
                    let exponent = if depth_range == 0 {
                        1.0
                    } else {
                        let t = 2.0 * f64::from(d_local) / f64::from(depth_range) - 1.0;
                        q.mul_add(t, 1.0)
                    };
                    if exponent == 0.0 { 1.0 } else { 0.0 }
                })
                .collect()
        } else if att.is_infinite() {
            (0..=depth_range)
                .map(|d_local| {
                    let exponent = if depth_range == 0 {
                        1.0
                    } else {
                        let t = 2.0 * f64::from(d_local) / f64::from(depth_range) - 1.0;
                        q.mul_add(t, 1.0)
                    };
                    if exponent == 0.0 {
                        1.0
                    } else if exponent > 0.0 {
                        f64::INFINITY
                    } else {
                        0.0
                    }
                })
                .collect()
        } else {
            let ln_att = att.ln();
            (0..=depth_range)
                .map(|d_local| {
                    let t = if depth_range == 0 {
                        0.0
                    } else {
                        2.0 * f64::from(d_local) / f64::from(depth_range) - 1.0
                    };
                    (ln_att * q.mul_add(t, 1.0)).exp()
                })
                .collect()
        };

        for &gid in &order {
            let d_local = self.gnode_depth(gid) - d_root;
            let factor = factors[d_local as usize];

            let new_own = self.gnodes.get(gid.index()).own.attenuate(factor);
            self.gnodes.get_mut(gid.index()).own = new_own;
        }

        gtree::recompute_g_sums_subtree(&mut self.gnodes, &order);

        if !is_global {
            if let Some(parent) = self.gnodes.get(root.index()).parent {
                gtree::recompute_g_sums(&mut self.gnodes, parent);
            }
        }

        for &gid in &order {
            let g = self.gnodes.get(gid.index());
            if let Some(v_id) = g.entry {
                let own = g.own;
                self.vnodes.get_mut(v_id.index()).intensity = own;
            }
        }

        if let Some(v_root) = self.v_root {
            vtree::recompute_all_v_intensities(&mut self.vnodes, v_root);
        }

        let _ = is_global;
        self.violations = rebalance::find_violated_nodes(&self.vnodes);
        let new_gnodes = rebalance::rebalance(
            &mut self.vnodes,
            &mut self.gnodes,
            &mut self.violations,
            self.live_depth_evict,
        );
        if !new_gnodes.is_empty() {
            self.handle_legacy_promotes(&new_gnodes);
        }

        self.plateau_recompute_sums("DECAY-SELECTIVE");

        #[cfg(feature = "dynamic-contour-tracking")]
        {
            self.plateaus_dirty = true;
        }
        self.normalize_plateaus();

        self.repair_p_i4();

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            self.debug_assert_plateau_mirror_consistency("POST-DECAY-SELECTIVE");
        }
    }
}
