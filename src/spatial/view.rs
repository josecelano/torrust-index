use crate::handle::GNodeId;
use crate::nodes::gnode::GState;
use crate::traits::{Accumulator, Coordinate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,
}

impl<C: Coordinate, V: Accumulator> Span<C, V> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cell<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,
}

impl<C: Coordinate, V: Accumulator> Cell<C, V> {
    #[inline]
    #[must_use]
    pub const fn to_span(self) -> Span<C, V> {
        Span {
            start: self.start,
            end: self.end,
            intensity: self.intensity,
            depth: self.depth,
        }
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }

    #[inline]
    #[must_use]
    pub fn is_final(&self, n: u32) -> bool {
        C::is_final(self.start, self.end, self.depth, n)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub own: V,

    pub sum: V,

    pub depth: u32,

    pub state: GState,

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
    use super::{Cell, Node, Span};
    use crate::handle::GNodeId;
    use crate::nodes::gnode::GState;
    use rstest::rstest;

    fn span(start: u8, end: u8, intensity: u32, depth: u32) -> Span<u8, u32> {
        Span {
            start,
            end,
            intensity,
            depth,
        }
    }

    fn cell(start: u8, end: u8, intensity: u32, depth: u32) -> Cell<u8, u32> {
        Cell {
            start,
            end,
            intensity,
            depth,
        }
    }

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

    // ── Span::width ──────────────────────────────────────────────────────
    mod span_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            assert_eq!(span(4, 12, 0, 0).width(), 8u8);
        }

        #[test]
        fn returns_zero_when_start_equals_end() {
            assert_eq!(span(5, 5, 0, 0).width(), 0u8);
        }
    }

    // ── Cell::to_span ────────────────────────────────────────────────────
    mod cell_to_span {
        use super::*;

        #[test]
        fn preserves_all_fields() {
            let c = cell(2, 10, 99, 3);
            let s = c.to_span();
            assert_eq!(s.start, 2u8);
            assert_eq!(s.end, 10u8);
            assert_eq!(s.intensity, 99u32);
            assert_eq!(s.depth, 3u32);
        }
    }

    // ── Cell::width ──────────────────────────────────────────────────────
    mod cell_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            assert_eq!(cell(0, 16, 0, 0).width(), 16u8);
        }
    }

    // ── Cell::is_final ───────────────────────────────────────────────────
    mod cell_is_final {
        use super::*;

        // For u8 coordinates: is_final returns true iff end - start == 1
        #[rstest]
        #[case(0u8, 1u8, 0u32, true)]
        #[case(0u8, 2u8, 0u32, false)]
        #[case(3u8, 4u8, 5u32, true)]
        #[case(0u8, 4u8, 0u32, false)]
        fn matches_coordinate_is_final(
            #[case] start: u8,
            #[case] end: u8,
            #[case] depth: u32,
            #[case] expected: bool,
        ) {
            let c = cell(start, end, 0, depth);
            assert_eq!(c.is_final(8), expected);
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
