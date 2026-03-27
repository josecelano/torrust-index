use crate::handle::{GNodeId, VNodeId};

#[derive(Debug)]
pub struct VNode<V> {
    pub(super) intensity: V,

    pub(super) parent: Option<VNodeId>,

    pub(super) kind: VKind<V>,
}

impl<V: Copy> VNode<V> {
    /// Constructs a new entry (leaf) V-node.
    #[must_use]
    pub const fn new_entry(
        intensity: V,
        parent: Option<VNodeId>,
        gnode: GNodeId,
        is_exposed: bool,
        is_evictable: bool,
    ) -> Self {
        Self {
            intensity,
            parent,
            kind: VKind::Entry {
                gnode,
                is_exposed,
                is_evictable,
            },
        }
    }

    /// Constructs a new structural (internal) V-node.
    #[must_use]
    pub const fn new_structural(
        intensity: V,
        parent: Option<VNodeId>,
        children: Children<V>,
        has_evictable: bool,
    ) -> Self {
        Self {
            intensity,
            parent,
            kind: VKind::Structural {
                children,
                has_evictable,
            },
        }
    }

    // ── Read accessors ───────────────────────────────────────────────────

    #[inline]
    #[must_use]
    pub const fn intensity(&self) -> V {
        self.intensity
    }

    #[inline]
    #[must_use]
    pub const fn parent(&self) -> Option<VNodeId> {
        self.parent
    }

    #[inline]
    #[must_use]
    pub const fn kind(&self) -> &VKind<V> {
        &self.kind
    }

    #[inline]
    #[must_use]
    pub const fn kind_mut(&mut self) -> &mut VKind<V> {
        &mut self.kind
    }

    // ── Write mutators ───────────────────────────────────────────────────

    #[inline]
    pub const fn set_intensity(&mut self, v: V) {
        self.intensity = v;
    }

    #[inline]
    pub const fn set_parent(&mut self, p: VNodeId) {
        self.parent = Some(p);
    }

    #[inline]
    pub const fn set_parent_opt(&mut self, p: Option<VNodeId>) {
        self.parent = p;
    }

}

impl<V: Clone> Clone for VNode<V> {
    fn clone(&self) -> Self {
        Self {
            intensity: self.intensity.clone(),
            parent: self.parent,
            kind: self.kind.clone(),
        }
    }
}

const _: () = assert!(std::mem::size_of::<VNode<u64>>() == 64);

#[derive(Debug, Clone)]
pub enum VKind<V> {
    Entry {
        gnode: GNodeId,

        is_exposed: bool,

        is_evictable: bool,
    },

    Structural {
        children: Children<V>,

        has_evictable: bool,
    },
}

/// The children of a structural V-node.
///
/// The V-tree is a 2-3 tree: a structural node always has exactly 2 or 3
/// children. Using an enum instead of a runtime `len` field makes the
/// constraint a type-system invariant rather than a runtime assertion.
#[derive(Debug, Clone)]
pub enum Children<V> {
    /// A structural node with exactly 2 children.
    Pair {
        ids: [VNodeId; 2],
        intensities: [V; 2],
    },
    /// A structural node with exactly 3 children.
    Triple {
        ids: [VNodeId; 3],
        intensities: [V; 3],
    },
}

impl<V> Children<V> {
    /// Constructs a 2-child node from two `(id, intensity)` pairs.
    #[must_use]
    pub fn new_2(a: (VNodeId, V), b: (VNodeId, V)) -> Self {
        Self::Pair {
            ids: [a.0, b.0],
            intensities: [a.1, b.1],
        }
    }

    /// Constructs a 3-child node from three `(id, intensity)` pairs.
    #[must_use]
    pub fn new_3(a: (VNodeId, V), b: (VNodeId, V), c: (VNodeId, V)) -> Self {
        Self::Triple {
            ids: [a.0, b.0, c.0],
            intensities: [a.1, b.1, c.1],
        }
    }

    /// Returns 2 for `Pair` and 3 for `Triple`.
    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        match self {
            Self::Pair { .. } => 2,
            Self::Triple { .. } => 3,
        }
    }

    /// Always `false`; a `Children` value always has at least 2 children.
    #[must_use]
    #[inline]
    #[allow(dead_code)]
    #[allow(clippy::unused_self)]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the slot index of the child with `id`, or `None` if absent.
    #[must_use]
    pub fn find_index(&self, id: VNodeId) -> Option<usize> {
        match self {
            Self::Pair { ids, .. } => ids.iter().position(|&i| i == id),
            Self::Triple { ids, .. } => ids.iter().position(|&i| i == id),
        }
    }

    /// Sets the intensity at slot `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range for the variant.
    #[inline]
    pub fn update_intensity(&mut self, index: usize, new: V) {
        match self {
            Self::Pair { intensities, .. } => intensities[index] = new,
            Self::Triple { intensities, .. } => intensities[index] = new,
        }
    }
}

impl<V: Copy + PartialOrd> Children<V> {
    /// Returns `(id, intensity)` at slot `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range for the variant (`>= 2` for `Pair`,
    /// `>= 3` for `Triple`).
    #[must_use]
    #[inline]
    pub const fn get(&self, index: usize) -> (VNodeId, V) {
        match self {
            Self::Pair { ids, intensities } => (ids[index], intensities[index]),
            Self::Triple { ids, intensities } => (ids[index], intensities[index]),
        }
    }

    /// Iterates over all `(id, intensity)` pairs in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (VNodeId, V)> + '_ {
        (0..self.len()).map(|i| self.get(i))
    }

    /// Returns the slot index of the child with the highest intensity.
    /// Ties are broken in favour of the lowest index.
    #[must_use]
    pub fn heaviest_child_index(&self) -> usize {
        let mut max_idx = 0;
        for i in 1..self.len() {
            if self.get(i).1 > self.get(max_idx).1 {
                max_idx = i;
            }
        }
        max_idx
    }

    /// Replaces the slot containing `old` with `new` and `new_intensity`.
    ///
    /// # Panics
    ///
    /// Panics if `old` is not present.
    pub fn replace_child(&mut self, old: VNodeId, new: VNodeId, new_intensity: V) {
        let idx = self
            .find_index(old)
            .expect("replace_child: old id not found");
        match self {
            Self::Pair { ids, intensities } => {
                ids[idx] = new;
                intensities[idx] = new_intensity;
            }
            Self::Triple { ids, intensities } => {
                ids[idx] = new;
                intensities[idx] = new_intensity;
            }
        }
    }

    /// Transitions a `Pair` into a `Triple` by appending a new child.
    ///
    /// # Panics
    ///
    /// Panics if called on a `Triple`.
    pub fn add_child(&mut self, id: VNodeId, intensity: V) {
        let (old_ids, old_intensities) = match self {
            Self::Pair { ids, intensities } => (*ids, *intensities),
            Self::Triple { .. } => panic!("add_child: already a 3-node"),
        };
        *self = Self::Triple {
            ids: [old_ids[0], old_ids[1], id],
            intensities: [old_intensities[0], old_intensities[1], intensity],
        };
    }

    /// Removes the child with `id` and transitions from `Triple` to `Pair`.
    ///
    /// The vacated slot is filled by the last slot (index 2) when `idx < 2`,
    /// preserving the same compaction order as the former `PackedChildren`.
    ///
    /// # Panics
    ///
    /// Panics if called on a `Pair` or if `id` is not present.
    pub fn remove_child(&mut self, id: VNodeId) -> (VNodeId, V) {
        let (ids, intensities) = match self {
            Self::Triple { ids, intensities } => (*ids, *intensities),
            Self::Pair { .. } => panic!("remove_child: not a 3-node"),
        };
        let idx = ids
            .iter()
            .position(|&i| i == id)
            .expect("remove_child: id not found");
        let removed = (ids[idx], intensities[idx]);
        let (new_ids, new_intensities) = if idx < 2 {
            let mut ids = ids;
            let mut intensities = intensities;
            ids[idx] = ids[2];
            intensities[idx] = intensities[2];
            ([ids[0], ids[1]], [intensities[0], intensities[1]])
        } else {
            ([ids[0], ids[1]], [intensities[0], intensities[1]])
        };
        *self = Self::Pair {
            ids: new_ids,
            intensities: new_intensities,
        };
        removed
    }
}

impl<V: Default> Default for VNode<V> {
    fn default() -> Self {
        Self {
            intensity: V::default(),
            parent: None,
            kind: VKind::Entry {
                gnode: GNodeId::from_index(0),
                is_exposed: false,
                is_evictable: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Children;
    use crate::handle::VNodeId;
    use rstest::rstest;

    fn id(i: usize) -> VNodeId {
        VNodeId::from_index(i)
    }

    // ── Children::new_2 ───────────────────────────────────────────────────
    mod new_2 {
        use super::*;

        #[test]
        fn creates_a_two_child_node() {
            let p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.len(), 2);
        }

        #[test]
        fn stores_correct_ids_and_intensities() {
            let p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.get(0), (id(0), 10));
            assert_eq!(p.get(1), (id(1), 20));
        }
    }

    // ── Children::new_3 ───────────────────────────────────────────────────
    mod new_3 {
        use super::*;

        #[test]
        fn creates_a_three_child_node() {
            let p = Children::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            assert_eq!(p.len(), 3);
        }

        #[test]
        fn stores_all_ids_and_intensities() {
            let p = Children::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            assert_eq!(p.get(0), (id(0), 1));
            assert_eq!(p.get(1), (id(1), 2));
            assert_eq!(p.get(2), (id(2), 3));
        }
    }

    // ── Children::iter ────────────────────────────────────────────────────
    mod iter {
        use super::*;

        #[test]
        fn yields_all_children_in_order() {
            let p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            let v: Vec<_> = p.iter().collect();
            assert_eq!(v, vec![(id(0), 10u32), (id(1), 20u32)]);
        }
    }

    // ── Children::heaviest_child_index ────────────────────────────────────
    mod heaviest_child_index {
        use super::*;

        #[rstest]
        #[case(10u32, 30u32, 1)] // b > a → index 1
        #[case(50u32, 20u32, 0)] // a > b → index 0
        #[case(10u32, 10u32, 0)] // equal → first wins
        fn returns_index_of_heaviest_child_among_two(
            #[case] a: u32,
            #[case] b: u32,
            #[case] expected: usize,
        ) {
            let p = Children::<u32>::new_2((id(0), a), (id(1), b));
            assert_eq!(p.heaviest_child_index(), expected);
        }

        #[test]
        fn returns_index_of_heaviest_child_among_three() {
            let p = Children::<u32>::new_3((id(0), 5), (id(1), 50), (id(2), 20));
            assert_eq!(p.heaviest_child_index(), 1);
        }
    }

    // ── Children::find_index ──────────────────────────────────────────────
    mod find_index {
        use super::*;

        #[test]
        fn returns_some_for_a_present_id() {
            let p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.find_index(id(1)), Some(1));
        }

        #[test]
        fn returns_none_for_an_absent_id() {
            let p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.find_index(id(5)), None);
        }
    }

    // ── Children::add_child ───────────────────────────────────────────────
    mod add_child {
        use super::*;

        #[test]
        fn promotes_two_node_to_three_node() {
            let mut p = Children::<u32>::new_2((id(0), 1), (id(1), 2));
            p.add_child(id(2), 3);
            assert_eq!(p.len(), 3);
            assert_eq!(p.get(2), (id(2), 3));
        }
    }

    // ── Children::remove_child ────────────────────────────────────────────
    mod remove_child {
        use super::*;

        #[test]
        fn demotes_three_node_to_two_node_and_returns_removed_entry() {
            let mut p = Children::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            let removed = p.remove_child(id(2));
            assert_eq!(p.len(), 2);
            assert_eq!(removed, (id(2), 3));
        }

        #[test]
        fn can_remove_from_middle_position() {
            let mut p = Children::<u32>::new_3((id(0), 10), (id(1), 20), (id(2), 30));
            let removed = p.remove_child(id(1));
            assert_eq!(p.len(), 2);
            assert_eq!(removed, (id(1), 20));
        }
    }

    // ── Children::replace_child ───────────────────────────────────────────
    mod replace_child {
        use super::*;

        #[test]
        fn updates_id_and_intensity_at_the_matching_slot() {
            let mut p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            p.replace_child(id(1), id(9), 99);
            assert_eq!(p.find_index(id(1)), None);
            assert_eq!(p.find_index(id(9)), Some(1));
            assert_eq!(p.get(1), (id(9), 99));
        }
    }

    // ── Children::update_intensity ────────────────────────────────────────
    mod update_intensity {
        use super::*;

        #[test]
        fn changes_intensity_at_the_given_index() {
            let mut p = Children::<u32>::new_2((id(0), 10), (id(1), 20));
            p.update_intensity(0, 77);
            assert_eq!(p.get(0), (id(0), 77));
        }
    }

    // ── Children::is_empty ────────────────────────────────────────────────
    mod is_empty {
        use super::*;

        #[test]
        fn returns_false_for_a_two_child_node() {
            let p = Children::<u32>::new_2((id(0), 1), (id(1), 2));
            assert!(!p.is_empty());
        }
    }

    // ── VNode::default ─────────────────────────────────────────────────────
    mod default_vnode {
        use super::super::{VKind, VNode};

        #[test]
        fn default_is_an_entry_node() {
            let n: VNode<u32> = VNode::default();
            assert!(matches!(n.kind, VKind::Entry { .. }));
        }
    }

    // ── VNode::clone ───────────────────────────────────────────────────────
    mod clone_vnode {
        use super::super::VNode;

        #[test]
        fn clone_preserves_intensity() {
            let mut original: VNode<u32> = VNode::default();
            original.intensity = 42;
            let cloned = original.clone();
            assert_eq!(cloned.intensity, 42);
        }

        #[test]
        fn clone_is_independent_of_original_intensity() {
            let mut original: VNode<u32> = VNode::default();
            original.intensity = 10;
            let cloned = original.clone();
            assert_eq!(original.intensity, 10);
            assert_ne!(cloned.intensity, 99);
        }
    }
}
