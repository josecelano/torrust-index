//! Display helpers for V-node debugging output.
//!
//! `Nd`, `Ch`, and `Ctx` are lightweight wrappers around arena references that
//! format a single node (or a node in context) for tracing and panic messages.
//! They are defined here rather than in `rebalance.rs` to keep that module
//! focused on algorithm logic.

use std::fmt;

use crate::arena::Arena;
use crate::handle::VNodeId;
use crate::nodes::vnode::{VKind, VNode};
use crate::traits::Accumulator;

use super::rebalance::max_uncle_intensity;

/// Formats a single V-node: `v{idx}(E,{intensity})` or `v{idx}(S{n},{intensity})`.
pub struct Nd<'a, V: Accumulator>(pub &'a Arena<VNode<V>>, pub VNodeId);

impl<V: Accumulator> fmt::Display for Nd<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        if !self.0.is_occupied(idx) {
            return write!(f, "v{idx}(DEAD)");
        }
        let n = self.0.get(idx);
        match &n.kind() {
            VKind::Entry { .. } => write!(f, "v{idx}(E,{:?})", n.intensity()),
            VKind::Structural { children, .. } => {
                write!(f, "v{idx}(S{},{:?})", children.len(), n.intensity())
            }
        }
    }
}

/// Formats the child list of a structural V-node: `[v{a}({ia}), v{b}({ib}), …]`.
pub(super) struct Ch<'a, V: Accumulator>(pub(super) &'a Arena<VNode<V>>, pub(super) VNodeId);

impl<V: Accumulator> fmt::Display for Ch<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.get(self.1.index()).kind() {
            VKind::Entry { .. } => f.write_str("∅"),
            VKind::Structural { children, .. } => {
                f.write_str("[")?;
                for i in 0..children.len() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    let (id, int) = children.get(i);
                    write!(f, "v{}({:?})", id.index(), int)?;
                }
                f.write_str("]")
            }
        }
    }
}

/// Formats a node together with its parent and grandparent context and (if
/// applicable) the maximum uncle intensity — useful for tracing why a node is
/// flagged as violated.
pub struct Ctx<'a, V: Accumulator>(pub &'a Arena<VNode<V>>, pub VNodeId);

impl<V: Accumulator> fmt::Display for Ctx<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (vnodes, c) = (self.0, self.1);
        write!(f, "{}", Nd(vnodes, c))?;
        let Some(p) = vnodes.get(c.index()).parent() else {
            return f.write_str(" (root)");
        };
        write!(f, " ← {} {}", Nd(vnodes, p), Ch(vnodes, p))?;
        if let Some(g) = vnodes.get(p.index()).parent() {
            write!(f, " ← {} {}", Nd(vnodes, g), Ch(vnodes, g))?;
        }
        if let Some(u) = max_uncle_intensity(vnodes, c) {
            write!(f, "  uncle_max={u:?}")?;
        }
        Ok(())
    }
}
