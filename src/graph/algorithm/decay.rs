use crate::graph::GvGraph;
use crate::graph::algorithm::rebalance;
use crate::handle::GNodeId;
use crate::traits::{Accumulator, Attenuatable, Coordinate, Inspectable};
use crate::tree::vtree;

#[allow(clippy::float_cmp)]
/// Compute per-depth attenuation factors for a decay operation over a G-tree
/// subtree that spans `depth_range + 1` depth levels.
///
/// For each local depth `d` in `0..=depth_range` the factor is:
///
/// $$f(d) = \text{att}^{q \cdot t + 1}, \qquad
///   t = \frac{2\,d}{D} - 1 \in [-1,\, 1]$$
///
/// where $D = \text{depth\_range}$ (or 0 when the subtree is a single level,
/// in which case $t = 0$ and every node receives exactly `att`).
///
/// The `q` parameter shapes the depth-selectivity:
/// - `q = 0.0` → **uniform**: every depth gets `att` unchanged.
/// - `q = 1.0` → **maximum taper**: the shallowest level ($t = -1$) receives
///   $\text{att}^0 = 1$ (no decay); the deepest level ($t = +1$) receives
///   $\text{att}^2$ (double-strength decay).
/// - Values between 0 and 1 interpolate linearly between those extremes.
///
/// Special cases for `att == 0.0` and `att == ∞` are handled explicitly to
/// avoid `NaN` arising from `0.0.ln()` and `∞.ln()`.
fn depth_attenuation_factors(att: f64, q: f64, depth_range: u32) -> Vec<f64> {
    // ── Special case: zero attenuation (0^exponent) ────────────────────────
    if att == 0.0 {
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
    // ── Special case: infinite attenuation (∞^exponent) ────────────────────
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
    // ── Normal case: finite positive attenuation (ln/exp path) ─────────────
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
    }
}

impl<C: Coordinate, V: Accumulator + Attenuatable + Inspectable, const N: u32> GvGraph<C, V, N> {
    /// Apply temporal decay to all G-nodes in the subtree rooted at `root`.
    ///
    /// # Panics
    ///
    /// Panics if `root` does not refer to a live G-node, if `attenuation` is
    /// negative or NaN, or if `q` is outside `[0.0, 1.0]` or NaN.
    pub fn decay(&mut self, root: GNodeId, attenuation: f64, q: f64) {
        let _span = tracing::debug_span!("decay", root = root.index(), attenuation, q,).entered();

        assert!(
            self.gtree.nodes.is_occupied(root.index()),
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

        let is_global = root == self.gtree.root;

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

            let g = self.gtree.nodes.get_mut(gid.index());
            g.set_own(g.own().attenuate(att));

            // NOTE: V-entry intensity is written inline here, alongside g.own(),
            // because the same factor applies to every depth.  The subsequent
            // `recompute_all_v_intensities` call derives all parent V-sums from
            // these leaf intensities, so inline updates are safe and avoid a
            // second tree traversal.
            if let Some(v_id) = g.entry() {
                let own = g.own();
                self.vnodes.get_mut(v_id.index()).set_intensity(own);
            }

            let g = self.gtree.nodes.get(gid.index());
            if let Some(left) = g.left() {
                stack.push(left);
            }
            if let Some(right) = g.right() {
                stack.push(right);
            }
        }

        self.gtree.recompute_sums_subtree(&order);

        if !is_global {
            if let Some(parent) = self.gtree.nodes.get(root.index()).parent() {
                self.gtree.recompute_sums(parent);
            }
        }

        if let Some(v_root) = self.v_root {
            vtree::recompute_all_v_intensities(&mut self.vnodes, v_root);
        }

        self.post_decay_repair("DECAY-UNIFORM");
    }

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
                let g = self.gtree.nodes.get(gid.index());
                if let Some(left) = g.left() {
                    stack.push(left);
                }
                if let Some(right) = g.right() {
                    stack.push(right);
                }
            }
        }
        let depth_range = max_depth - d_root;

        let factors = depth_attenuation_factors(att, q, depth_range);

        for &gid in &order {
            let d_local = self.gnode_depth(gid) - d_root;
            let factor = factors[d_local as usize];

            let new_own = self.gtree.nodes.get(gid.index()).own().attenuate(factor);
            self.gtree.nodes.get_mut(gid.index()).set_own(new_own);
        }

        self.gtree.recompute_sums_subtree(&order);

        if !is_global {
            if let Some(parent) = self.gtree.nodes.get(root.index()).parent() {
                self.gtree.recompute_sums(parent);
            }
        }

        // NOTE: V-entry intensities are written in a *separate second pass*,
        // after all G-node own-values have been updated and G-sums recomputed.
        // This is required because the per-depth factors differ: if we wrote
        // v.intensity() inline (as decay_uniform does), a node at depth d would
        // receive a factor derived from the not-yet-final g.own() of a sibling at
        // a different depth.  Delaying until all g.own() are stable avoids that
        // ordering hazard.
        for &gid in &order {
            let g = self.gtree.nodes.get(gid.index());
            if let Some(v_id) = g.entry() {
                let own = g.own();
                self.vnodes.get_mut(v_id.index()).set_intensity(own);
            }
        }

        if let Some(v_root) = self.v_root {
            vtree::recompute_all_v_intensities(&mut self.vnodes, v_root);
        }

        self.post_decay_repair("DECAY-SELECTIVE");
    }

    fn post_decay_repair(&mut self, label: &str) {
        self.violations = rebalance::find_violated_nodes(&self.vnodes);
        let new_gnodes = rebalance::rebalance(
            &mut self.vnodes,
            &mut self.gtree.nodes,
            &mut self.violations,
            self.gtree.live_depth_evict,
        );
        if !new_gnodes.is_empty() {
            self.handle_legacy_promotes(&new_gnodes);
        }

        self.plateau_recompute_sums(label);

        #[cfg(feature = "dynamic-contour-tracking")]
        {
            self.plateaus_dirty = true;
        }
        self.normalize_plateaus();

        self.repair_p_i4();

        #[cfg(feature = "dynamic-contour-tracking")]
        if cfg!(debug_assertions) || tracing::enabled!(tracing::Level::DEBUG) {
            let post_label = format!("POST-{label}");
            self.debug_assert_plateau_mirror_consistency(&post_label);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph, StructuralConfig};

    type G = GvGraph<u8, u32, 8>;

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            structural: StructuralConfig {
                depth_create: 3,
                depth_evict: 5,
                budget: None,
                alpha_relax: 0.5,
                bounded_eviction: false,
            },
        }
    }

    fn observed_graph() -> G {
        let mut g = GvGraph::new(make_config());
        g.observe(0u8, 100u32);
        g
    }

    // ── decay (uniform, q = 0) ───────────────────────────────────────────
    mod decay_uniform {
        use super::*;

        #[test]
        fn attenuation_of_one_is_no_op() {
            let mut g = observed_graph();
            let before = g.total_sum();
            let root = g.gtree.root;
            g.decay(root, 1.0, 0.0);
            assert_eq!(g.total_sum(), before);
        }

        #[test]
        fn attenuation_reduces_total_sum() {
            let mut g = observed_graph();
            let before = g.total_sum();
            let root = g.gtree.root;
            g.decay(root, 0.5, 0.0);
            assert!(g.total_sum() <= before);
        }

        #[test]
        fn zero_attenuation_drives_sum_to_zero() {
            let mut g = observed_graph();
            let root = g.gtree.root;
            g.decay(root, 0.0, 0.0);
            assert_eq!(g.total_sum(), 0u32);
        }
    }

    // ── decay (selective, q > 0) ─────────────────────────────────────────
    mod decay_selective {
        use super::*;

        #[test]
        fn attenuation_of_one_is_no_op() {
            let mut g = observed_graph();
            let before = g.total_sum();
            let root = g.gtree.root;
            g.decay(root, 1.0, 0.5);
            assert_eq!(g.total_sum(), before);
        }

        #[test]
        fn attenuation_reduces_total_sum() {
            let mut g = observed_graph();
            let before = g.total_sum();
            let root = g.gtree.root;
            g.decay(root, 0.5, 0.5);
            assert!(g.total_sum() <= before);
        }

        #[test]
        fn zero_attenuation_selective_drives_sum_to_zero() {
            let mut g = observed_graph();
            let root = g.gtree.root;
            g.decay(root, 0.0, 0.5);
            assert_eq!(g.total_sum(), 0u32);
        }

        #[test]
        fn infinite_attenuation_selective_does_not_panic() {
            let mut g = observed_graph();
            let root = g.gtree.root;
            // inf attenuation zeroes the sum (positive exponents → factor=∞ → own.attenuate(∞) = 0)
            g.decay(root, f64::INFINITY, 0.5);
        }

        #[test]
        fn multi_depth_selective_decay_covers_t_computation() {
            // Need depth_range > 0; trigger a split so the subtree has two levels.
            let mut g: G = GvGraph::new(make_config());
            for _ in 0..3 {
                g.observe(64u8, 5u32); // bootstrap split
            }
            for _ in 0..3 {
                g.observe(32u8, 5u32); // second split
            }
            let root = g.gtree.root;
            g.decay(root, 0.5, 0.5);
        }

        #[test]
        fn zero_att_depth_range_zero_covers_if_depth_range_zero_branch() {
            // depth_range == 0 with att == 0.0: exercises the `if depth_range == 0
            // { 1.0 }` branch in the att==0.0 arm (line 140).
            // Fresh graph has one Terminal gnode → depth_range = 0.
            let mut g: G = GvGraph::new(make_config());
            let root = g.gtree.root;
            g.decay(root, 0.0, 0.5);
            assert_eq!(g.total_sum(), 0u32);
        }

        #[test]
        fn infinite_att_depth_range_zero_covers_if_depth_range_zero_branch() {
            // depth_range == 0 with att == INFINITY: exercises the `if depth_range
            // == 0 { 1.0 }` branch in the att.is_infinite() arm (line 152).
            let mut g: G = GvGraph::new(make_config());
            let root = g.gtree.root;
            g.decay(root, f64::INFINITY, 0.5);
        }

        #[test]
        fn infinite_att_q_one_zero_exponent_branch() {
            // att == INFINITY, q == 1.0, depth_range > 0: at d_local=0
            // t = -1.0, exponent = q * t + 1 = 0.0 → exercises the
            // `if exponent == 0.0 { 1.0 }` branch (line 158).
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32); // bootstrap split → depth_range = 1
            let root = g.gtree.root;
            g.decay(root, f64::INFINITY, 1.0);
        }

        #[test]
        fn normal_att_depth_range_zero_covers_if_depth_range_zero_branch() {
            // depth_range == 0 with a regular att: exercises the `if depth_range
            // == 0 { 0.0 }` branch in the standard ln-based arm (line 171).
            let mut g: G = GvGraph::new(make_config());
            g.observe(0u8, 1u32); // sum=1 but no split (1 <= threshold=2)
            let root = g.gtree.root;
            g.decay(root, 0.5, 0.5);
        }
    }

    // ── sub-root decay (is_global = false) ───────────────────────────────
    mod decay_non_global {
        use super::*;

        fn split_graph() -> G {
            let mut g: G = GvGraph::new(make_config());
            for _ in 0..3 {
                g.observe(64u8, 5u32);
            }
            g
        }

        #[test]
        fn sub_root_uniform_decay_does_not_panic() {
            let mut g = split_graph();
            let children = g.gnode_children(g.gtree.root).unwrap();
            // Decay a child subtree (non-global) to exercise the is_global=false path
            if let Some(sub_root) = children.left.or_else(|| children.right) {
                g.decay(sub_root, 0.5, 0.0);
            }
        }

        #[test]
        fn sub_root_selective_decay_does_not_panic() {
            let mut g = split_graph();
            let children = g.gnode_children(g.gtree.root).unwrap();
            if let Some(sub_root) = children.left.or_else(|| children.right) {
                g.decay(sub_root, 0.5, 0.5);
            }
        }
    }
}
