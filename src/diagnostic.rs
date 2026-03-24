#![allow(dead_code)]

use std::fmt;

use crate::arena::Arena;
use crate::gnode::GNode;
use crate::handle::{GNodeId, VNodeId};
use crate::rebalance::{self, Ctx};
use crate::traits::{Accumulator, Coordinate, Inspectable};
use crate::vnode::{VKind, VNode};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::{gnode::GState, graph::GvGraph};

pub fn audit_violations<V: Accumulator + Inspectable>(
    vnodes: &Arena<VNode<V>>,
    violations: &[VNodeId],
    checkpoint: &str,
) -> Vec<VNodeId> {
    let all_violated = rebalance::find_violated_nodes(vnodes);
    let queued: std::collections::HashSet<usize> = violations.iter().map(|v| v.index()).collect();
    let mut missed = Vec::new();
    for &v in &all_violated {
        if !queued.contains(&v.index()) {
            tracing::error!(
                checkpoint,
                node = %Ctx(vnodes, v),
                "violation NOT in queue",
            );
            missed.push(v);
        }
    }
    missed
}

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
                tracing::error!(checkpoint, ?key, gnode = r.index(), ?back, "basis back-pointer inconsistency");
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

pub struct EvictionContext {
    pub evicted_parent: Option<VNodeId>,
    pub evicted_parent_child_count: usize,
    pub collapse_sibling: Option<VNodeId>,
}

#[allow(clippy::too_many_lines)]
pub fn diagnose_missed_violation<V: Accumulator + Inspectable>(
    vnodes: &Arena<VNode<V>>,
    violated: VNodeId,
    context: &EvictionContext,
) {
    let v = vnodes.get(violated.index());
    let v_intensity = v.intensity;

    let Some(parent_id) = v.parent else {
        tracing::error!(
            node = violated.index(),
            "DIAGNOSIS: node has no parent (root?), should not be violated",
        );
        return;
    };

    let parent = vnodes.get(parent_id.index());
    let Some(grandparent_id) = parent.parent else {
        tracing::error!(
            node = violated.index(),
            parent = parent_id.index(),
            "DIAGNOSIS: parent has no grandparent (depth 1?), should not be violated",
        );
        return;
    };

    let grandparent = vnodes.get(grandparent_id.index());
    let uncles: Vec<(VNodeId, V)> = match &grandparent.kind {
        VKind::Structural { children, .. } => children.iter().filter(|(id, _)| *id != parent_id).collect(),
        VKind::Entry { .. } => vec![],
    };

    let max_uncle_intensity = uncles.iter().map(|(_, int)| int.to_f64_approx()).fold(0.0_f64, f64::max);

    let uncle_desc: Vec<String> = uncles
        .iter()
        .map(|(id, int)| format!("v{}({})", id.index(), int.to_f64_approx()))
        .collect();

    tracing::error!(
        node = violated.index(),
        intensity = v_intensity.to_f64_approx(),
        parent = parent_id.index(),
        parent_intensity = parent.intensity.to_f64_approx(),
        grandparent = grandparent_id.index(),
        grandparent_intensity = grandparent.intensity.to_f64_approx(),
        ?uncle_desc,
        max_uncle = max_uncle_intensity,
        is_violation = v_intensity.to_f64_approx() > max_uncle_intensity,
        "MISSED VIOLATION DIAGNOSIS",
    );

    tracing::error!(
        evicted_parent = ?context.evicted_parent.map(VNodeId::index),
        child_count = context.evicted_parent_child_count,
        collapse_sibling = ?context.collapse_sibling.map(VNodeId::index),
        "eviction context (child_count: 2=collapse, 3=3→2)",
    );

    if let Some(sole) = context.collapse_sibling {
        if is_ancestor(vnodes, sole, violated) {
            tracing::error!(
                node = violated.index(),
                collapse_sibling = sole.index(),
                "node IS a descendant of collapse_sibling → should have been caught by source 7",
            );

            let sole_children: Vec<usize> = match &vnodes.get(sole.index()).kind {
                VKind::Structural { children, .. } => (0..children.len()).map(|i| children.get(i).0.index()).collect(),
                VKind::Entry { .. } => vec![],
            };
            if sole_children.contains(&violated.index()) {
                tracing::error!(
                    node = violated.index(),
                    "node IS a direct child of collapse_sibling — source 7 should catch it",
                );
            } else {
                tracing::error!(
                    node = violated.index(),
                    ?sole_children,
                    "node is NOT a direct child — source 7 only checks direct children. MISSING SOURCE.",
                );
            }
        } else {
            tracing::error!(
                node = violated.index(),
                collapse_sibling = sole.index(),
                "node is NOT a descendant of collapse_sibling",
            );
        }
    }

    if let Some(evicted_p) = context.evicted_parent {
        if evicted_p == grandparent_id {
            tracing::error!(
                node = violated.index(),
                evicted_parent = evicted_p.index(),
                "node's grandparent = evicted_parent → parent is sibling of removed entry. Check source 8.",
            );
        }
    }

    let _ancestry_span = tracing::error_span!("vtree_path_to_violated", node = violated.index()).entered();
    let mut current = violated;
    let mut depth = 0_usize;
    loop {
        let n = vnodes.get(current.index());
        let kind = match &n.kind {
            VKind::Entry { .. } => "E",
            VKind::Structural { children, .. } => match children.len() {
                2 => "S2",
                3 => "S3",
                _ => "S?",
            },
        };
        tracing::error!(
            depth,
            node = current.index(),
            kind,
            intensity = n.intensity.to_f64_approx(),
            "ancestry",
        );
        match n.parent {
            Some(p) => {
                current = p;
                depth += 1;
            }
            None => break,
        }
    }
}

fn is_ancestor<V: Accumulator>(vnodes: &Arena<VNode<V>>, ancestor: VNodeId, mut descendant: VNodeId) -> bool {
    while let Some(p) = vnodes.get(descendant.index()).parent {
        if p == ancestor {
            return true;
        }
        descendant = p;
    }
    false
}

pub struct Gn<'a, C: Coordinate, V: Accumulator + Inspectable>(pub &'a Arena<GNode<C, V>>, pub GNodeId);

impl<C: Coordinate, V: Accumulator + Inspectable> fmt::Display for Gn<'_, C, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        if !self.0.is_occupied(idx) {
            return write!(f, "G{idx}(DEAD)");
        }
        let g = self.0.get(idx);
        let state = match g.state() {
            crate::gnode::GState::Terminal => "T",
            crate::gnode::GState::SemiInternal => "S",
            crate::gnode::GState::Internal => "I",
        };
        write!(
            f,
            "G{idx}({state},[{},{}),sum={})",
            g.lo.to_f64(),
            g.hi.to_f64(),
            g.sum.to_f64_approx()
        )
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub struct Pl<'a, C: Coordinate, V: Accumulator + Inspectable, const N: u32>(pub &'a GvGraph<C, V, N>, pub GNodeId);

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> fmt::Display for Pl<'_, C, V, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        let key = self.0.plateau_basis.plateau_key(self.1);
        match key {
            Some(k) => {
                if let Some(plateau) = self.0.plateaus.get(&k) {
                    write!(
                        f,
                        "P([{},{}),d={},sum={})",
                        plateau.start.to_f64(),
                        plateau.end.to_f64(),
                        plateau.depth,
                        plateau.sum.to_f64_approx()
                    )
                } else {
                    write!(f, "P(G{idx},key={k:?},NO_PLATEAU)")
                }
            }
            None => write!(f, "P(G{idx},not_basis)"),
        }
    }
}
