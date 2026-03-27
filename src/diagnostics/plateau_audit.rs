#[cfg(feature = "dynamic-contour-tracking")]
use crate::graph::GvGraph;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::handle::GNodeId;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::nodes::gnode::GState;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::traits::{Accumulator, Coordinate, Inspectable};

#[cfg(feature = "dynamic-contour-tracking")]
pub struct PlateauAuditContext {
    pub parent_id: GNodeId,
    pub parent_state: GState,
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn audit_plateau_consistency<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    checkpoint: &str,
    context: Option<&PlateauAuditContext>,
) {
    for &key in graph.plateaus.keys() {
        for &r in graph.plateau_basis.basis_elements(&key) {
            let back = graph.plateau_basis.plateau_key(r);
            if back != Some(key) {
                tracing::error!(
                    checkpoint,
                    ?key,
                    gnode = r.index(),
                    ?back,
                    "basis back-pointer inconsistency"
                );
            }
        }
    }

    let map_len = graph.plateaus.len();
    let basis_len = graph.plateau_basis.plateau_count();
    if map_len != basis_len {
        tracing::error!(
            checkpoint,
            map_len,
            basis_len,
            "plateaus.len() != plateau_basis.plateau_count()",
        );
    }

    if let Some(ctx) = context {
        if ctx.parent_state == GState::SemiInternal {
            let g = graph.gnodes.get(ctx.parent_id.index());
            let surviving = g.left.or(g.right);
            if let Some(surviving_id) = surviving {
                let parent_plateau = graph.plateau_basis.plateau_key(ctx.parent_id);
                if let Some(pk) = parent_plateau {
                    if let Some(ck) = graph.plateau_basis.plateau_key(surviving_id) {
                        if pk == ck {
                            tracing::debug!(
                                checkpoint,
                                ?pk,
                                ?ck,
                                parent = ctx.parent_id.index(),
                                surviving = surviving_id.index(),
                                "transient P-I4 overlap (will be repaired)",
                            );
                        }
                    }
                }
            }
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

    #[cfg(feature = "dynamic-contour-tracking")]
    mod audit_plateau_consistency_fn {
        use super::*;
        use crate::diagnostics::plateau_audit::{PlateauAuditContext, audit_plateau_consistency};
        use crate::handle::GNodeId;
        use crate::nodes::gnode::GState;

        #[test]
        fn does_not_panic_for_fresh_graph_no_context() {
            let g: G = GvGraph::new(make_config());
            audit_plateau_consistency(&g, "test", None);
        }

        #[test]
        fn does_not_panic_after_bootstrap_no_context() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            audit_plateau_consistency(&g, "test", None);
        }

        #[test]
        fn with_semi_internal_context_on_terminal_leaf() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Use a leaf gnode (Terminal) as context parent with SemiInternal state.
            // surviving = leaf.left.or(leaf.right) = None → inner block skipped.
            let ctx = PlateauAuditContext {
                parent_id: GNodeId::from_index(1), // leaf gnode
                parent_state: GState::SemiInternal,
            };
            audit_plateau_consistency(&g, "test", Some(&ctx));
        }

        #[test]
        fn with_semi_internal_context_on_internal_gnode() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Use the Internal root (gnode 0) as context parent with SemiInternal.
            // surviving = root.left.or(root.right) = Some(left_child).
            let ctx = PlateauAuditContext {
                parent_id: GNodeId::from_index(0), // internal root gnode
                parent_state: GState::SemiInternal,
            };
            audit_plateau_consistency(&g, "test", Some(&ctx));
        }
    }
}
