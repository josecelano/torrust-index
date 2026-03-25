use std::cmp::Ordering;
#[cfg(feature = "dynamic-contour-tracking")]
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(feature = "dynamic-contour-tracking")]
use std::sync::OnceLock;

#[cfg(feature = "dynamic-contour-tracking")]
use crate::handle::GNodeId;
use crate::nodes::gnode::{GNode, GState};
use crate::spatial::view::Span;
use crate::traits::{Accumulator, Coordinate};

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

#[cfg(feature = "dynamic-contour-tracking")]
#[derive(Debug, Clone)]
pub struct PlateauBasis<C: Coordinate> {
    forward: BTreeMap<BasisEdge<C>, HashSet<GNodeId>>,

    back: HashMap<GNodeId, BasisEdge<C>>,
}

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate> PlateauBasis<C> {
    pub(crate) fn new() -> Self {
        Self {
            forward: BTreeMap::new(),
            back: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, key: BasisEdge<C>, gnode: GNodeId) {
        debug_assert!(
            !self.back.contains_key(&gnode),
            "PlateauBasis::insert: gnode {gnode:?} already in \
             basis of plateau {:?}",
            self.back.get(&gnode)
        );
        self.forward.entry(key).or_default().insert(gnode);
        self.back.insert(gnode, key);
    }

    pub(crate) fn remove(&mut self, gnode: GNodeId) -> Option<BasisEdge<C>> {
        let key = self.back.remove(&gnode)?;
        if let Some(set) = self.forward.get_mut(&key) {
            set.remove(&gnode);
            if set.is_empty() {
                self.forward.remove(&key);
            }
        }
        Some(key)
    }

    #[inline]
    pub(crate) fn plateau_key(&self, gnode: GNodeId) -> Option<BasisEdge<C>> {
        self.back.get(&gnode).copied()
    }

    pub(crate) fn basis_elements(&self, key: &BasisEdge<C>) -> &HashSet<GNodeId> {
        static EMPTY: OnceLock<HashSet<GNodeId>> = OnceLock::new();
        self.forward
            .get(key)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }

    #[inline]
    #[allow(unused)]
    pub(crate) fn contains(&self, gnode: GNodeId) -> bool {
        self.back.contains_key(&gnode)
    }

    #[inline]
    pub(crate) fn plateau_count(&self) -> usize {
        self.forward.len()
    }

    #[inline]
    pub(crate) fn basis_count(&self) -> usize {
        self.back.len()
    }

    #[allow(unused)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&BasisEdge<C>, &HashSet<GNodeId>)> {
        self.forward.iter()
    }

    pub(crate) const fn back_map(&self) -> &HashMap<GNodeId, BasisEdge<C>> {
        &self.back
    }

    pub(crate) fn rebuild(
        &mut self,
        assignments: impl IntoIterator<Item = (BasisEdge<C>, GNodeId)>,
    ) {
        self.forward.clear();
        self.back.clear();
        for (key, gid) in assignments {
            self.forward.entry(key).or_default().insert(gid);
            self.back.insert(gid, key);
        }
    }
}

pub fn basis_edge_of<C, V>(g: &GNode<C, V>) -> BasisEdge<C>
where
    C: Coordinate,
    V: Accumulator,
{
    match g.state() {
        GState::Terminal | GState::Internal => BasisEdge(g.lo),
        GState::SemiInternal => {
            if g.left.is_some() {
                BasisEdge(C::midpoint(g.lo, g.hi))
            } else {
                BasisEdge(g.lo)
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

    // ── PlateauBasis (dynamic-contour-tracking) ──────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod plateau_basis_tests {
        use super::*;
        use crate::spatial::plateau::PlateauBasis;

        fn make_basis() -> PlateauBasis<u8> {
            PlateauBasis::new()
        }

        #[test]
        fn new_basis_is_empty() {
            let pb = make_basis();
            assert_eq!(pb.plateau_count(), 0);
            assert_eq!(pb.basis_count(), 0);
        }

        #[test]
        fn insert_increments_counts() {
            let mut pb = make_basis();
            let gid = GNodeId::from_index(0);
            pb.insert(BasisEdge(10u8), gid);
            assert_eq!(pb.plateau_count(), 1);
            assert_eq!(pb.basis_count(), 1);
        }

        #[test]
        fn contains_returns_true_for_inserted_gnode() {
            let mut pb = make_basis();
            let gid = GNodeId::from_index(0);
            pb.insert(BasisEdge(5u8), gid);
            assert!(pb.contains(gid));
        }

        #[test]
        fn contains_returns_false_for_absent_gnode() {
            let pb = make_basis();
            assert!(!pb.contains(GNodeId::from_index(0)));
        }

        #[test]
        fn basis_elements_returns_empty_for_unknown_key() {
            let pb = make_basis();
            // Key never inserted → should return the static empty set
            assert!(pb.basis_elements(&BasisEdge(99u8)).is_empty());
        }

        #[test]
        fn iter_yields_all_inserted_entries() {
            let mut pb = make_basis();
            pb.insert(BasisEdge(1u8), GNodeId::from_index(0));
            pb.insert(BasisEdge(2u8), GNodeId::from_index(1));
            let count = pb.iter().count();
            assert_eq!(count, 2);
        }

        #[test]
        fn back_map_contains_inserted_gnode() {
            let mut pb = make_basis();
            let gid = GNodeId::from_index(7);
            pb.insert(BasisEdge(3u8), gid);
            assert!(pb.back_map().contains_key(&gid));
        }

        #[test]
        fn remove_existing_gnode_decrements_counts() {
            let mut pb = make_basis();
            let gid = GNodeId::from_index(0);
            pb.insert(BasisEdge(10u8), gid);
            let removed = pb.remove(gid);
            assert!(removed.is_some());
            assert_eq!(pb.basis_count(), 0);
        }

        #[test]
        fn remove_absent_gnode_returns_none() {
            let mut pb = make_basis();
            assert!(pb.remove(GNodeId::from_index(42)).is_none());
        }

        #[test]
        fn plateau_key_returns_correct_key() {
            let mut pb = make_basis();
            let gid = GNodeId::from_index(0);
            let key = BasisEdge(50u8);
            pb.insert(key, gid);
            assert_eq!(pb.plateau_key(gid), Some(key));
        }
    }
}
