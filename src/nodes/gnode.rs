use crate::handle::{GNodeId, VNodeId};

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GNodeChildren {
    pub left: Option<GNodeId>,

    pub right: Option<GNodeId>,
}

#[derive(Debug, Clone)]
pub struct GNode<C, V> {
    pub(super) lo: C,

    pub(super) hi: C,

    pub(super) sum: V,

    pub(super) own: V,

    pub(super) left: Option<GNodeId>,

    pub(super) right: Option<GNodeId>,

    pub(super) parent: Option<GNodeId>,

    pub(super) entry: Option<VNodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GState {
    Terminal,

    SemiInternal,

    Internal,
}

impl<C: Copy, V: Copy> GNode<C, V> {
    /// Constructs a new leaf G-node.
    #[must_use]
    pub const fn new_leaf(lo: C, hi: C, zero: V, parent: Option<GNodeId>) -> Self {
        Self {
            lo,
            hi,
            sum: zero,
            own: zero,
            left: None,
            right: None,
            parent,
            entry: None,
        }
    }

    // ── Read accessors ───────────────────────────────────────────────────

    #[inline]
    #[must_use]
    pub const fn lo(&self) -> C {
        self.lo
    }

    #[inline]
    #[must_use]
    pub const fn hi(&self) -> C {
        self.hi
    }

    #[inline]
    #[must_use]
    pub const fn sum(&self) -> V {
        self.sum
    }

    #[inline]
    #[must_use]
    pub const fn own(&self) -> V {
        self.own
    }

    #[inline]
    #[must_use]
    pub const fn left(&self) -> Option<GNodeId> {
        self.left
    }

    #[inline]
    #[must_use]
    pub const fn right(&self) -> Option<GNodeId> {
        self.right
    }

    #[inline]
    #[must_use]
    pub const fn parent(&self) -> Option<GNodeId> {
        self.parent
    }

    #[inline]
    #[must_use]
    pub const fn entry(&self) -> Option<VNodeId> {
        self.entry
    }

    // ── Write mutators ───────────────────────────────────────────────────

    #[inline]
    pub const fn set_sum(&mut self, v: V) {
        self.sum = v;
    }

    #[inline]
    pub const fn set_own(&mut self, v: V) {
        self.own = v;
    }

    #[inline]
    pub const fn link_left(&mut self, l: GNodeId) {
        self.left = Some(l);
    }

    #[inline]
    pub const fn link_right(&mut self, r: GNodeId) {
        self.right = Some(r);
    }

    /// Clears a specific child slot: if `child` matches `left`, clears `left`;
    /// if it matches `right`, clears `right`; otherwise panics.
    #[inline]
    pub fn clear_child(&mut self, child: GNodeId) {
        if self.left == Some(child) {
            self.left = None;
        } else if self.right == Some(child) {
            self.right = None;
        } else {
            panic!("clear_child: {child:?} is not a child of this node");
        }
    }

    #[inline]
    pub const fn assign_entry(&mut self, vid: VNodeId) {
        self.entry = Some(vid);
    }

    #[inline]
    pub const fn clear_entry(&mut self) {
        self.entry = None;
    }
}

impl<C, V> GNode<C, V> {
    #[inline]
    #[must_use]
    pub const fn state(&self) -> GState {
        match (self.left, self.right) {
            (None, None) => GState::Terminal,
            (Some(_), Some(_)) => GState::Internal,
            _ => GState::SemiInternal,
        }
    }

    #[inline]
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn has_dependents(&self) -> bool {
        self.left.is_some() || self.right.is_some()
    }

    #[inline]
    #[must_use]
    pub const fn is_semi_internal(&self) -> bool {
        matches!(self.state(), GState::SemiInternal)
    }

    #[must_use]
    pub fn uncovered_range(&self) -> Option<(C, C)>
    where
        C: crate::traits::Coordinate,
    {
        match (self.left, self.right) {
            (None, None) => Some((self.lo, self.hi)),
            (Some(_), None) => {
                let mid = C::midpoint(self.lo, self.hi);
                Some((mid, self.hi))
            }
            (None, Some(_)) => {
                let mid = C::midpoint(self.lo, self.hi);
                Some((self.lo, mid))
            }
            (Some(_), Some(_)) => None,
        }
    }
}

impl<C: Default, V: Default> Default for GNode<C, V> {
    fn default() -> Self {
        Self {
            lo: C::default(),
            hi: C::default(),
            sum: V::default(),
            own: V::default(),
            left: None,
            right: None,
            parent: None,
            entry: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GNode, GState};
    use crate::handle::GNodeId;

    fn make_node(left: Option<GNodeId>, right: Option<GNodeId>) -> GNode<u8, u32> {
        GNode {
            lo: 0,
            hi: 16,
            sum: 0,
            own: 0,
            left,
            right,
            parent: None,
            entry: None,
        }
    }

    // ── GNode::state ────────────────────────────────────────────────────
    mod state {
        use super::*;

        #[test]
        fn is_terminal_when_both_children_are_none() {
            assert_eq!(make_node(None, None).state(), GState::Terminal);
        }

        #[test]
        fn is_internal_when_both_children_are_some() {
            let l = GNodeId::from_index(1);
            let r = GNodeId::from_index(2);
            assert_eq!(make_node(Some(l), Some(r)).state(), GState::Internal);
        }

        #[test]
        fn is_semi_internal_when_only_left_child_exists() {
            let l = GNodeId::from_index(1);
            assert_eq!(make_node(Some(l), None).state(), GState::SemiInternal);
        }

        #[test]
        fn is_semi_internal_when_only_right_child_exists() {
            let r = GNodeId::from_index(1);
            assert_eq!(make_node(None, Some(r)).state(), GState::SemiInternal);
        }
    }

    // ── GNode::is_terminal ──────────────────────────────────────────────
    mod is_terminal {
        use super::*;

        #[test]
        fn returns_true_when_no_children() {
            assert!(make_node(None, None).is_terminal());
        }

        #[test]
        fn returns_false_when_left_child_is_present() {
            let l = GNodeId::from_index(1);
            assert!(!make_node(Some(l), None).is_terminal());
        }
    }

    // ── GNode::is_semi_internal ─────────────────────────────────────────
    mod is_semi_internal {
        use super::*;

        #[test]
        fn returns_true_with_only_left_child() {
            let l = GNodeId::from_index(1);
            assert!(make_node(Some(l), None).is_semi_internal());
        }

        #[test]
        fn returns_true_with_only_right_child() {
            let r = GNodeId::from_index(1);
            assert!(make_node(None, Some(r)).is_semi_internal());
        }

        #[test]
        fn returns_false_for_terminal_node() {
            assert!(!make_node(None, None).is_semi_internal());
        }

        #[test]
        fn returns_false_for_internal_node() {
            let l = GNodeId::from_index(1);
            let r = GNodeId::from_index(2);
            assert!(!make_node(Some(l), Some(r)).is_semi_internal());
        }
    }

    // ── GNode::uncovered_range ─────────────────────────────────────────
    mod uncovered_range {
        use super::*;

        #[test]
        fn returns_full_range_for_terminal_node() {
            let n = make_node(None, None);
            assert_eq!(n.uncovered_range(), Some((0u8, 16u8)));
        }

        #[test]
        fn returns_right_half_when_only_left_child_exists() {
            // midpoint(0, 16) == 8; uncovered side is [8, 16)
            let l = GNodeId::from_index(1);
            let n = make_node(Some(l), None);
            assert_eq!(n.uncovered_range(), Some((8u8, 16u8)));
        }

        #[test]
        fn returns_left_half_when_only_right_child_exists() {
            // midpoint(0, 16) == 8; uncovered side is [0, 8)
            let r = GNodeId::from_index(1);
            let n = make_node(None, Some(r));
            assert_eq!(n.uncovered_range(), Some((0u8, 8u8)));
        }

        #[test]
        fn returns_none_for_fully_internal_node() {
            let l = GNodeId::from_index(1);
            let r = GNodeId::from_index(2);
            let n = make_node(Some(l), Some(r));
            assert_eq!(n.uncovered_range(), None);
        }
    }

    // ── GNode::default ─────────────────────────────────────────────────
    mod default {
        use super::*;

        #[test]
        fn produces_a_zero_node_with_no_children() {
            let n: GNode<u8, u32> = GNode::default();
            assert_eq!(n.lo, 0u8);
            assert_eq!(n.sum, 0u32);
            assert!(n.left.is_none() && n.right.is_none());
        }
    }

    // ── GNode::has_dependents ───────────────────────────────────────────
    mod has_dependents {
        use super::*;

        #[test]
        fn returns_false_for_terminal_node() {
            assert!(!make_node(None, None).has_dependents());
        }

        #[test]
        fn returns_true_when_left_child_exists() {
            let l = GNodeId::from_index(1);
            assert!(make_node(Some(l), None).has_dependents());
        }

        #[test]
        fn returns_true_when_right_child_exists() {
            let r = GNodeId::from_index(1);
            assert!(make_node(None, Some(r)).has_dependents());
        }
    }
}
