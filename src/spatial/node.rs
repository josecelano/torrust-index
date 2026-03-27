use crate::handle::GNodeId;
use crate::nodes::gnode::GState;
use crate::spatial::view::{Cell, Span};
use crate::traits::{Accumulator, Coordinate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub own: V,

    pub sum: V,

    pub depth: u32,

    pub state: GState,

    // TODO(opportunity-8): Node exposes gnode_id (an internal handle);
    // consider replacing with an opaque query result in a later phase.
    pub gnode_id: GNodeId,

    pub parent: Option<GNodeId>,
}

impl<C: Coordinate, V: Accumulator> Node<C, V> {
    #[inline]
    #[must_use]
    pub const fn to_span(self) -> Span<C, V> {
        Span {
            start: self.start,
            end: self.end,
            intensity: self.sum,
            depth: self.depth,
        }
    }

    #[inline]
    #[must_use]
    pub fn to_cell(self) -> Option<Cell<C, V>> {
        if self.state == GState::Terminal {
            Some(Cell {
                start: self.start,
                end: self.end,
                intensity: self.own,
                depth: self.depth,
            })
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.state == GState::Terminal
    }

    #[inline]
    #[must_use]
    pub fn refinement(&self) -> V {
        V::sub(self.sum, self.own)
    }

    #[inline]
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::Node;
    use crate::handle::GNodeId;
    use crate::nodes::gnode::GState;
    use rstest::rstest;

    fn node(start: u8, end: u8, own: u32, sum: u32, depth: u32, state: GState) -> Node<u8, u32> {
        Node {
            start,
            end,
            own,
            sum,
            depth,
            state,
            gnode_id: GNodeId::from_index(0),
            parent: None,
        }
    }

    // ── Node::to_span ────────────────────────────────────────────────────
    mod node_to_span {
        use super::*;

        #[test]
        fn uses_sum_as_intensity() {
            let n = node(0, 16, 10, 50, 2, GState::Terminal);
            let s = n.to_span();
            assert_eq!(s.start, 0u8);
            assert_eq!(s.end, 16u8);
            assert_eq!(s.intensity, 50u32);
            assert_eq!(s.depth, 2u32);
        }
    }

    // ── Node::to_cell ────────────────────────────────────────────────────
    mod node_to_cell {
        use super::*;

        #[test]
        fn returns_some_with_own_as_intensity_for_terminal_state() {
            let n = node(0, 16, 10, 50, 1, GState::Terminal);
            let c = n.to_cell().unwrap();
            assert_eq!(c.intensity, 10u32); // uses own, not sum
        }

        #[rstest]
        #[case(GState::Internal)]
        #[case(GState::SemiInternal)]
        fn returns_none_for_non_terminal_state(#[case] state: GState) {
            let n = node(0, 16, 10, 50, 1, state);
            assert!(n.to_cell().is_none());
        }
    }

    // ── Node::is_terminal ────────────────────────────────────────────────
    mod node_is_terminal {
        use super::*;

        #[rstest]
        #[case(GState::Terminal, true)]
        #[case(GState::Internal, false)]
        #[case(GState::SemiInternal, false)]
        fn matches_state(#[case] state: GState, #[case] expected: bool) {
            let n = node(0, 16, 0, 0, 0, state);
            assert_eq!(n.is_terminal(), expected);
        }
    }

    // ── Node::refinement ─────────────────────────────────────────────────
    mod node_refinement {
        use super::*;

        #[test]
        fn returns_sum_minus_own() {
            let n = node(0, 16, 30, 100, 0, GState::Internal);
            assert_eq!(n.refinement(), 70u32);
        }

        #[test]
        fn returns_zero_when_sum_equals_own() {
            let n = node(0, 16, 50, 50, 0, GState::Terminal);
            assert_eq!(n.refinement(), 0u32);
        }
    }

    // ── Node::is_root ────────────────────────────────────────────────────
    mod node_is_root {
        use super::*;

        #[test]
        fn returns_true_when_parent_is_none() {
            let n = node(0, 16, 0, 0, 0, GState::Terminal);
            assert!(n.is_root());
        }

        #[test]
        fn returns_false_when_parent_is_some() {
            let mut n = node(0, 16, 0, 0, 0, GState::Terminal);
            n.parent = Some(GNodeId::from_index(1));
            assert!(!n.is_root());
        }
    }

    // ── Node::width ──────────────────────────────────────────────────────
    mod node_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            let n = node(4, 20, 0, 0, 0, GState::Terminal);
            assert_eq!(n.width(), 16u8);
        }
    }
}
