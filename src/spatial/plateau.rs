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
