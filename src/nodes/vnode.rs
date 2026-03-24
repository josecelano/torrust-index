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
