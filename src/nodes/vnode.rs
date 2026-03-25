use std::sync::atomic::{AtomicU32, Ordering};

use crate::handle::{GNodeId, VNodeId};

pub const DEPTH_STALE: u32 = u32::MAX;

#[derive(Debug)]
pub struct VNode<V> {
    pub intensity: V,

    pub parent: Option<VNodeId>,

    pub cached_depth: AtomicU32,

    pub kind: VKind<V>,
}

impl<V: Clone> Clone for VNode<V> {
    fn clone(&self) -> Self {
        Self {
            intensity: self.intensity.clone(),
            parent: self.parent,
            cached_depth: AtomicU32::new(self.cached_depth.load(Ordering::Relaxed)),
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
        children: PackedChildren<V>,

        has_evictable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct PackedChildren<V> {
    pub intensities: [V; 3],

    pub ids: [Option<VNodeId>; 3],

    pub len: u8,
}

impl<V: Copy + Default + PartialOrd> PackedChildren<V> {
    #[must_use]
    pub fn new_2(a: (VNodeId, V), b: (VNodeId, V)) -> Self {
        Self {
            intensities: [a.1, b.1, V::default()],
            ids: [Some(a.0), Some(b.0), None],
            len: 2,
        }
    }

    #[must_use]
    pub const fn new_3(a: (VNodeId, V), b: (VNodeId, V), c: (VNodeId, V)) -> Self {
        Self {
            intensities: [a.1, b.1, c.1],
            ids: [Some(a.0), Some(b.0), Some(c.0)],
            len: 3,
        }
    }

    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    #[inline]
    #[allow(dead_code)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    #[inline]
    pub fn get(&self, index: usize) -> (VNodeId, V) {
        assert!(index < self.len(), "PackedChildren::get out of bounds");
        (
            self.ids[index].expect("child ID should be Some within len"),
            self.intensities[index],
        )
    }

    pub fn iter(&self) -> impl Iterator<Item = (VNodeId, V)> + '_ {
        (0..self.len()).map(|i| self.get(i))
    }

    #[must_use]
    pub fn heaviest_child_index(&self) -> usize {
        let mut max_idx = 0;
        for i in 1..self.len() {
            if self.intensities[i] > self.intensities[max_idx] {
                max_idx = i;
            }
        }
        max_idx
    }

    #[must_use]
    pub fn find_index(&self, id: VNodeId) -> Option<usize> {
        (0..self.len()).find(|&i| self.ids[i] == Some(id))
    }

    pub fn replace_child(&mut self, old: VNodeId, new: VNodeId, new_intensity: V) {
        let idx = self
            .find_index(old)
            .expect("replace_child: old id not found");
        self.ids[idx] = Some(new);
        self.intensities[idx] = new_intensity;
    }

    pub fn add_child(&mut self, id: VNodeId, intensity: V) {
        assert!(self.len == 2, "add_child: already a 3-node");
        self.ids[2] = Some(id);
        self.intensities[2] = intensity;
        self.len = 3;
    }

    pub fn remove_child(&mut self, id: VNodeId) -> (VNodeId, V) {
        assert!(self.len == 3, "remove_child: not a 3-node");
        let idx = self.find_index(id).expect("remove_child: id not found");
        let removed = self.get(idx);

        if idx < 2 {
            self.ids[idx] = self.ids[2];
            self.intensities[idx] = self.intensities[2];
        }
        self.ids[2] = None;
        self.intensities[2] = V::default();
        self.len = 2;
        removed
    }

    #[inline]
    pub fn update_intensity(&mut self, index: usize, new: V) {
        debug_assert!(index < self.len());
        self.intensities[index] = new;
    }
}

impl<V: Default> Default for VNode<V> {
    fn default() -> Self {
        Self {
            intensity: V::default(),
            parent: None,
            cached_depth: AtomicU32::new(DEPTH_STALE),
            kind: VKind::Entry {
                gnode: GNodeId::from_index(0),
                is_exposed: false,
                is_evictable: false,
            },
        }
    }
}

impl<V: Default + Copy> Default for PackedChildren<V> {
    fn default() -> Self {
        Self {
            intensities: [V::default(); 3],
            ids: [None; 3],
            len: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackedChildren;
    use crate::handle::VNodeId;
    use rstest::rstest;

    fn id(i: usize) -> VNodeId {
        VNodeId::from_index(i)
    }

    // ── PackedChildren::new_2 ─────────────────────────────────────────────
    mod new_2 {
        use super::*;

        #[test]
        fn creates_a_two_child_node() {
            let p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.len(), 2);
        }

        #[test]
        fn stores_correct_ids_and_intensities() {
            let p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.get(0), (id(0), 10));
            assert_eq!(p.get(1), (id(1), 20));
        }
    }

    // ── PackedChildren::new_3 ─────────────────────────────────────────────
    mod new_3 {
        use super::*;

        #[test]
        fn creates_a_three_child_node() {
            let p = PackedChildren::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            assert_eq!(p.len(), 3);
        }

        #[test]
        fn stores_all_ids_and_intensities() {
            let p = PackedChildren::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            assert_eq!(p.get(0), (id(0), 1));
            assert_eq!(p.get(1), (id(1), 2));
            assert_eq!(p.get(2), (id(2), 3));
        }
    }

    // ── PackedChildren::iter ───────────────────────────────────────────────
    mod iter {
        use super::*;

        #[test]
        fn yields_all_children_in_order() {
            let p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            let v: Vec<_> = p.iter().collect();
            assert_eq!(v, vec![(id(0), 10u32), (id(1), 20u32)]);
        }
    }

    // ── PackedChildren::heaviest_child_index ─────────────────────────────
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
            let p = PackedChildren::<u32>::new_2((id(0), a), (id(1), b));
            assert_eq!(p.heaviest_child_index(), expected);
        }

        #[test]
        fn returns_index_of_heaviest_child_among_three() {
            let p = PackedChildren::<u32>::new_3((id(0), 5), (id(1), 50), (id(2), 20));
            assert_eq!(p.heaviest_child_index(), 1);
        }
    }

    // ── PackedChildren::find_index ──────────────────────────────────────────
    mod find_index {
        use super::*;

        #[test]
        fn returns_some_for_a_present_id() {
            let p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.find_index(id(1)), Some(1));
        }

        #[test]
        fn returns_none_for_an_absent_id() {
            let p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            assert_eq!(p.find_index(id(5)), None);
        }
    }

    // ── PackedChildren::add_child ──────────────────────────────────────────
    mod add_child {
        use super::*;

        #[test]
        fn promotes_two_node_to_three_node() {
            let mut p = PackedChildren::<u32>::new_2((id(0), 1), (id(1), 2));
            p.add_child(id(2), 3);
            assert_eq!(p.len(), 3);
            assert_eq!(p.get(2), (id(2), 3));
        }
    }

    // ── PackedChildren::remove_child ──────────────────────────────────────
    mod remove_child {
        use super::*;

        #[test]
        fn demotes_three_node_to_two_node_and_returns_removed_entry() {
            let mut p = PackedChildren::<u32>::new_3((id(0), 1), (id(1), 2), (id(2), 3));
            let removed = p.remove_child(id(2));
            assert_eq!(p.len(), 2);
            assert_eq!(removed, (id(2), 3));
        }

        #[test]
        fn can_remove_from_middle_position() {
            let mut p = PackedChildren::<u32>::new_3((id(0), 10), (id(1), 20), (id(2), 30));
            let removed = p.remove_child(id(1));
            assert_eq!(p.len(), 2);
            assert_eq!(removed, (id(1), 20));
        }
    }

    // ── PackedChildren::replace_child ────────────────────────────────────
    mod replace_child {
        use super::*;

        #[test]
        fn updates_id_and_intensity_at_the_matching_slot() {
            let mut p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            p.replace_child(id(1), id(9), 99);
            assert_eq!(p.find_index(id(1)), None);
            assert_eq!(p.find_index(id(9)), Some(1));
            assert_eq!(p.get(1), (id(9), 99));
        }
    }

    // ── PackedChildren::update_intensity ───────────────────────────────
    mod update_intensity {
        use super::*;

        #[test]
        fn changes_intensity_at_the_given_index() {
            let mut p = PackedChildren::<u32>::new_2((id(0), 10), (id(1), 20));
            p.update_intensity(0, 77);
            assert_eq!(p.get(0), (id(0), 77));
        }
    }
    // ── PackedChildren::is_empty ───────────────────────────────────────────
    mod is_empty {
        use super::*;

        #[test]
        fn returns_false_for_a_two_child_node() {
            let p = PackedChildren::<u32>::new_2((id(0), 1), (id(1), 2));
            assert!(!p.is_empty());
        }

        #[test]
        fn returns_true_for_default_children() {
            let p: PackedChildren<u32> = PackedChildren::default();
            assert!(p.is_empty());
        }
    }

    // ── PackedChildren::default ────────────────────────────────────────────
    mod default_packed_children {
        use super::*;

        #[test]
        fn starts_with_len_zero() {
            let p: PackedChildren<u32> = PackedChildren::default();
            assert_eq!(p.len(), 0);
        }
    }

    // ── VNode::default ─────────────────────────────────────────────────────
    mod default_vnode {
        use super::super::{VKind, VNode, DEPTH_STALE};
        use std::sync::atomic::Ordering;

        #[test]
        fn default_is_an_entry_node_with_stale_depth() {
            let n: VNode<u32> = VNode::default();
            assert!(matches!(n.kind, VKind::Entry { .. }));
            assert_eq!(n.cached_depth.load(Ordering::Relaxed), DEPTH_STALE);
        }
    }

    // ── VNode::clone ───────────────────────────────────────────────────────
    mod clone_vnode {
        use super::super::{VNode, DEPTH_STALE};
        use std::sync::atomic::Ordering;

        #[test]
        fn clone_preserves_intensity_and_cached_depth() {
            let mut original: VNode<u32> = VNode::default();
            original.intensity = 42;
            original.cached_depth.store(3, Ordering::Relaxed);
            let cloned = original.clone();
            assert_eq!(cloned.intensity, 42);
            assert_eq!(cloned.cached_depth.load(Ordering::Relaxed), 3);
        }

        #[test]
        fn clone_is_independent_of_original_depth() {
            let original: VNode<u32> = VNode::default();
            let cloned = original.clone();
            cloned.cached_depth.store(99, Ordering::Relaxed);
            assert_eq!(original.cached_depth.load(Ordering::Relaxed), DEPTH_STALE);
        }
    }}
