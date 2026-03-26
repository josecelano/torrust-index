#[cfg(feature = "dynamic-contour-tracking")]
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(feature = "dynamic-contour-tracking")]
use std::sync::OnceLock;

#[cfg(feature = "dynamic-contour-tracking")]
use crate::handle::GNodeId;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau::BasisEdge;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::traits::Coordinate;

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

#[cfg(test)]
#[cfg(feature = "dynamic-contour-tracking")]
mod plateau_basis_tests {
    use super::PlateauBasis;
    use crate::handle::GNodeId;
    use crate::spatial::plateau::BasisEdge;

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
