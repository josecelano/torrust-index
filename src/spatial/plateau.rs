use crate::nodes::gnode::{GNode, GState};
use crate::spatial::view::Span;
use crate::traits::{Accumulator, Coordinate};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BasisEdge<C: Coordinate>(pub C);

impl<C: Coordinate> Ord for BasisEdge<C> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl<C: Coordinate> PartialOrd for BasisEdge<C> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<C: Coordinate> PartialEq for BasisEdge<C> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<C: Coordinate> Eq for BasisEdge<C> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Plateau<C: Coordinate, V: Accumulator> {
    pub basis_edge: BasisEdge<C>,

    pub start: C,

    pub end: C,

    pub depth: u32,

    pub sum: V,
}

impl<C: Coordinate, V: Accumulator> Plateau<C, V> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }

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
}

pub fn basis_edge_of<C, V>(g: &GNode<C, V>) -> BasisEdge<C>
where
    C: Coordinate,
    V: Accumulator,
{
    match g.state() {
        GState::Terminal | GState::Internal => BasisEdge(g.lo()),
        GState::SemiInternal => {
            if g.left().is_some() {
                BasisEdge(C::midpoint(g.lo(), g.hi()))
            } else {
                BasisEdge(g.lo())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BasisEdge, Plateau, basis_edge_of};
    use crate::handle::GNodeId;
    use crate::nodes::gnode::GNode;
    use rstest::rstest;
    use std::cmp::Ordering;

    fn make_gnode(left: Option<GNodeId>, right: Option<GNodeId>) -> GNode<u8, u32> {
        let mut g = GNode::new_leaf(0u8, 16u8, 0u32, None);
        if let Some(l) = left { g.link_left(l); }
        if let Some(r) = right { g.link_right(r); }
        g
    }

    // ── BasisEdge ordering ───────────────────────────────────────────────
    mod basis_edge_ordering {
        use super::*;

        #[rstest]
        #[case(BasisEdge(1u8), BasisEdge(2u8), Ordering::Less)]
        #[case(BasisEdge(2u8), BasisEdge(2u8), Ordering::Equal)]
        #[case(BasisEdge(3u8), BasisEdge(2u8), Ordering::Greater)]
        fn total_order_via_coordinate_total_cmp(
            #[case] a: BasisEdge<u8>,
            #[case] b: BasisEdge<u8>,
            #[case] expected: Ordering,
        ) {
            assert_eq!(a.cmp(&b), expected);
        }

        #[test]
        fn equal_basis_edges_compare_equal() {
            assert_eq!(BasisEdge(5u8), BasisEdge(5u8));
        }

        #[test]
        fn less_is_less() {
            assert!(BasisEdge(1u8) < BasisEdge(2u8));
        }

        #[test]
        fn greater_is_greater() {
            assert!(BasisEdge(3u8) > BasisEdge(2u8));
        }
    }

    // ── Plateau::width ───────────────────────────────────────────────────
    mod plateau_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            let p = Plateau::<u8, u32> {
                basis_edge: BasisEdge(0u8),
                start: 4,
                end: 20,
                depth: 1,
                sum: 100,
            };
            assert_eq!(p.width(), 16u8);
        }
    }

    // ── Plateau::to_span ─────────────────────────────────────────────────
    mod plateau_to_span {
        use super::*;

        #[test]
        fn uses_sum_as_intensity() {
            let p = Plateau::<u8, u32> {
                basis_edge: BasisEdge(0u8),
                start: 2,
                end: 10,
                depth: 3,
                sum: 77,
            };
            let s = p.to_span();
            assert_eq!(s.start, 2u8);
            assert_eq!(s.end, 10u8);
            assert_eq!(s.intensity, 77u32);
            assert_eq!(s.depth, 3u32);
        }
    }

    // ── basis_edge_of ────────────────────────────────────────────────────
    mod basis_edge_of_fn {
        use super::*;

        #[test]
        fn terminal_node_uses_lo() {
            let g = make_gnode(None, None);
            assert_eq!(basis_edge_of(&g), BasisEdge(0u8));
        }

        #[test]
        fn internal_node_uses_lo() {
            let l = GNodeId::from_index(1);
            let r = GNodeId::from_index(2);
            let g = make_gnode(Some(l), Some(r));
            assert_eq!(basis_edge_of(&g), BasisEdge(0u8));
        }

        #[test]
        fn semi_internal_with_left_child_uses_midpoint() {
            let l = GNodeId::from_index(1);
            let g = make_gnode(Some(l), None);
            // midpoint(0, 16) = 8
            assert_eq!(basis_edge_of(&g), BasisEdge(8u8));
        }

        #[test]
        fn semi_internal_with_right_child_uses_lo() {
            let r = GNodeId::from_index(1);
            let g = make_gnode(None, Some(r));
            assert_eq!(basis_edge_of(&g), BasisEdge(0u8));
        }
    }
}
