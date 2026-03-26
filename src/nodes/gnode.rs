use crate::handle::{GNodeId, VNodeId};

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GNodeChildren {
    pub left: Option<GNodeId>,

    pub right: Option<GNodeId>,
}

#[derive(Debug, Clone)]
pub struct GNode<C, V> {
    pub(crate) lo: C,

    pub(crate) hi: C,

    pub(crate) sum: V,

    pub(crate) own: V,

    pub(crate) left: Option<GNodeId>,

    pub(crate) right: Option<GNodeId>,

    pub(crate) parent: Option<GNodeId>,

    pub(crate) entry: Option<VNodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GState {
    Terminal,

    SemiInternal,

    Internal,
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
