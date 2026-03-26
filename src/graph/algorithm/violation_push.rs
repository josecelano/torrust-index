//! Violation-push helpers — all functions that push violated `VNodeId`s into
//! a `violations` vector based on structural conditions in the V-tree.
//!
//! These are a cohesive family unrelated to the rebalancing algorithm itself.
//! Callers in `rebalance.rs` import them via `use super::violation_push::*`.

use crate::arena::Arena;
use crate::handle::VNodeId;
use crate::nodes::vnode::{VKind, VNode};
use crate::traits::Accumulator;

use super::rebalance::{Nd, is_violated};
use super::violation_sources::ViolationSources;

// ── Plain wrappers (use all-enabled config) ──────────────────────────────────

pub fn push_side_effect_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_side_effect_violations_with_config(
        vnodes,
        node,
        violations,
        ViolationSources::all_enabled(),
    );
}

pub fn push_source_10_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_source_10_violations_with_config(
        vnodes,
        node,
        violations,
        ViolationSources::all_enabled(),
    );
}

pub fn push_promoted_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_promoted_violations_with_config(vnodes, node, violations, ViolationSources::all_enabled());
}

pub fn push_contraction_child_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    skip: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    let child_ids: Vec<VNodeId> = match &vnodes.get(node.index()).kind {
        VKind::Structural { children, .. } => children.iter().map(|(id, _)| id).collect(),
        VKind::Entry { .. } => return,
    };
    for child_id in child_ids {
        if child_id != skip && is_violated(vnodes, child_id) {
            tracing::trace!(
                child = %Nd(vnodes, child_id),
                skip = skip.index(),
                "contraction-child violation",
            );
            violations.push(child_id);
        }
    }
}

pub fn push_leaf_removal_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    start: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_leaf_removal_violations_with_config(
        vnodes,
        start,
        violations,
        ViolationSources::all_enabled(),
    );
}

pub fn push_collapse_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    sole: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_collapse_violations_with_config(vnodes, sole, violations, ViolationSources::all_enabled());
}

pub fn push_remaining_sibling_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    p: VNodeId,
    removed: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_remaining_sibling_violations_with_config(
        vnodes,
        p,
        removed,
        violations,
        ViolationSources::all_enabled(),
    );
}

pub fn push_cousin_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    sole: VNodeId,
    grandparent: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    push_cousin_violations_with_config(
        vnodes,
        sole,
        grandparent,
        violations,
        ViolationSources::all_enabled(),
    );
}

// ── Config variants ──────────────────────────────────────────────────────────

#[inline]
pub fn push_side_effect_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_3_contraction_grandchildren {
        return;
    }
    push_grandchild_violations(vnodes, node, violations);
}

#[inline]
pub fn push_source_10_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_10_g_contraction_promotion {
        return;
    }
    push_grandchild_violations(vnodes, node, violations);
}

#[inline]
pub fn push_promoted_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_4_promotion_children {
        return;
    }
    let child_ids: Vec<VNodeId> = match &vnodes.get(node.index()).kind {
        VKind::Structural { children, .. } => children.iter().map(|(id, _)| id).collect(),
        VKind::Entry { .. } => return,
    };
    for child_id in child_ids {
        if is_violated(vnodes, child_id) {
            tracing::trace!(child = %Nd(vnodes, child_id), at = %Nd(vnodes, node), "promoted violation");
            violations.push(child_id);
        }
    }
}

#[inline]
pub fn push_leaf_removal_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    start: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_6_leaf_removal_ancestors {
        return;
    }
    let mut ancestor = start;
    while let Some(parent) = vnodes.get(ancestor.index()).parent {
        let sibling_ids: Vec<VNodeId> = match &vnodes.get(parent.index()).kind {
            VKind::Structural { children, .. } => children
                .iter()
                .map(|(id, _)| id)
                .filter(|&id| id != ancestor)
                .collect(),
            VKind::Entry { .. } => break,
        };

        for sib_id in sibling_ids {
            if let VKind::Structural { children, .. } = &vnodes.get(sib_id.index()).kind {
                for i in 0..children.len() {
                    let (child_id, _) = children.get(i);
                    if is_violated(vnodes, child_id) {
                        tracing::trace!(
                            child = %Nd(vnodes, child_id),
                            weakened_uncle = %Nd(vnodes, ancestor),
                            "leaf-removal violation",
                        );
                        violations.push(child_id);
                    }
                }
            }
        }

        ancestor = parent;
    }
}

#[inline]
pub fn push_collapse_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    sole: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_7_collapse_children {
        return;
    }
    tracing::debug!(
        sole = sole.index(),
        "push_collapse_violations: checking node"
    );
    push_children_violations(vnodes, sole, "collapse (source 7)", violations);
}

#[inline]
pub fn push_remaining_sibling_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    p: VNodeId,
    removed: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_8_three_to_two_siblings {
        return;
    }

    let remaining: Vec<VNodeId> = match &vnodes.get(p.index()).kind {
        VKind::Structural { children, .. } => children
            .iter()
            .map(|(id, _)| id)
            .filter(|&id| id != removed)
            .collect(),
        VKind::Entry { .. } => return,
    };

    for sibling in remaining {
        push_children_violations(vnodes, sibling, "3→2 transition (source 8)", violations);
    }
}

#[inline]
pub fn push_cousin_violations_with_config<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    sole: VNodeId,
    grandparent: VNodeId,
    violations: &mut Vec<VNodeId>,
    config: ViolationSources,
) {
    if !config.source_9_collapse_cousins {
        return;
    }

    let cousins: Vec<VNodeId> = match &vnodes.get(grandparent.index()).kind {
        VKind::Structural { children, .. } => children
            .iter()
            .map(|(id, _)| id)
            .filter(|&id| id != sole)
            .collect(),
        VKind::Entry { .. } => return,
    };

    tracing::debug!(
        sole = sole.index(),
        grandparent = grandparent.index(),
        cousins = ?cousins.iter().map(|c| c.index()).collect::<Vec<_>>(),
        "push_cousin_violations: checking cousins' children (source 9)",
    );

    for cousin in cousins {
        push_children_violations(vnodes, cousin, "collapse cousins (source 9)", violations);
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn push_grandchild_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    violations: &mut Vec<VNodeId>,
) {
    let child_ids: Vec<VNodeId> = match &vnodes.get(node.index()).kind {
        VKind::Structural { children, .. } => children.iter().map(|(id, _)| id).collect(),
        VKind::Entry { .. } => return,
    };
    for child_id in child_ids {
        if let VKind::Structural { children, .. } = &vnodes.get(child_id.index()).kind {
            for i in 0..children.len() {
                let (gc_id, _) = children.get(i);
                if is_violated(vnodes, gc_id) {
                    tracing::trace!(gc = %Nd(vnodes, gc_id), at = %Nd(vnodes, node), "side-effect violation");
                    violations.push(gc_id);
                }
            }
        }
    }
}

fn push_children_violations<V: Accumulator>(
    vnodes: &Arena<VNode<V>>,
    node: VNodeId,
    source: &str,
    violations: &mut Vec<VNodeId>,
) {
    let children: Vec<VNodeId> = match &vnodes.get(node.index()).kind {
        VKind::Structural { children, .. } => {
            (0..children.len()).map(|i| children.get(i).0).collect()
        }
        VKind::Entry { .. } => {
            tracing::debug!(
                node = node.index(),
                source,
                "push_children_violations: node is entry, no children",
            );
            return;
        }
    };

    tracing::debug!(
        node = node.index(),
        children = ?children.iter().map(|c| c.index()).collect::<Vec<_>>(),
        source,
        "push_children_violations: checking children",
    );

    for child_id in children {
        let violated = is_violated(vnodes, child_id);
        tracing::debug!(
            child = child_id.index(),
            violated,
            source,
            "push_children_violations: child check",
        );
        if violated {
            tracing::trace!(
                child = %Nd(vnodes, child_id),
                parent = %Nd(vnodes, node),
                source,
                "sibling-removal violation",
            );
            violations.push(child_id);
        }
    }
}
