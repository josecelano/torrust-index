use std::fmt;

pub struct Arena<T: Default> {
    slots: Vec<T>,

    occupied: Vec<u64>,

    free: Vec<u32>,

    count: u32,
}

impl<T: Default> Arena<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            occupied: Vec::new(),
            free: Vec::new(),
            count: 0,
        }
    }

    #[must_use]
    #[inline]
    pub const fn count(&self) -> u32 {
        self.count
    }

    pub fn alloc(&mut self, value: T) -> usize {
        let index = if let Some(idx) = self.free.pop() {
            let i = idx as usize;
            self.slots[i] = value;
            i
        } else {
            let i = self.slots.len();
            self.slots.push(value);

            let word = i / 64;
            if word >= self.occupied.len() {
                self.occupied.resize(word + 1, 0);
            }
            i
        };
        self.set_occupied(index, true);
        self.count += 1;
        index
    }

    pub fn dealloc(&mut self, index: usize) -> T {
        debug_assert!(
            self.is_occupied(index),
            "Arena::dealloc: slot {index} is not occupied"
        );
        self.set_occupied(index, false);
        #[allow(clippy::cast_possible_truncation)]
        self.free.push(index as u32);
        self.count -= 1;
        std::mem::take(&mut self.slots[index])
    }

    #[must_use]
    #[inline]
    pub fn get(&self, index: usize) -> &T {
        debug_assert!(
            self.is_occupied(index),
            "Arena::get: slot {index} is not occupied"
        );
        &self.slots[index]
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(
            self.is_occupied(index),
            "Arena::get_mut: slot {index} is not occupied"
        );
        &mut self.slots[index]
    }

    #[must_use]
    #[inline]
    pub fn is_occupied(&self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;
        word < self.occupied.len() && (self.occupied[word] & (1u64 << bit)) != 0
    }

    pub fn iter_occupied(&self) -> impl Iterator<Item = (usize, &T)> + '_ {
        (0..self.slots.len())
            .filter(move |&i| self.is_occupied(i))
            .map(move |i| (i, &self.slots[i]))
    }

    fn set_occupied(&mut self, index: usize, value: bool) {
        let word = index / 64;
        let bit = index % 64;
        if value {
            self.occupied[word] |= 1u64 << bit;
        } else {
            self.occupied[word] &= !(1u64 << bit);
        }
    }
}

impl<T: Default> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default + fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Arena")
            .field("count", &self.count)
            .field("capacity", &self.slots.len())
            .field("free_list_len", &self.free.len())
            .finish_non_exhaustive()
    }
}

impl<T: Default + Clone> Clone for Arena<T> {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            occupied: self.occupied.clone(),
            free: self.free.clone(),
            count: self.count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Arena;

    // ── Arena::new ───────────────────────────────────────────────────────
    mod new {
        use super::*;

        #[test]
        fn starts_empty() {
            let a: Arena<u32> = Arena::new();
            assert_eq!(a.count(), 0);
        }
    }

    // ── Arena::alloc ────────────────────────────────────────────────────
    mod alloc {
        use super::*;

        #[test]
        fn increases_count_by_one() {
            let mut a: Arena<u32> = Arena::new();
            a.alloc(42);
            assert_eq!(a.count(), 1);
        }

        #[test]
        fn returns_incrementing_indices_when_no_free_slots() {
            let mut a: Arena<u32> = Arena::new();
            let i0 = a.alloc(1);
            let i1 = a.alloc(2);
            let i2 = a.alloc(3);
            assert_eq!((i0, i1, i2), (0, 1, 2));
        }

        #[test]
        fn reuses_freed_slot() {
            let mut a: Arena<u32> = Arena::new();
            let i0 = a.alloc(10);
            a.dealloc(i0);
            let i1 = a.alloc(20);
            assert_eq!(i0, i1);
        }
    }

    // ── Arena::get ─────────────────────────────────────────────────────
    mod get {
        use super::*;

        #[test]
        fn returns_the_stored_value() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(99);
            assert_eq!(*a.get(i), 99);
        }
    }

    // ── Arena::get_mut ──────────────────────────────────────────────────
    mod get_mut {
        use super::*;

        #[test]
        fn allows_mutation_in_place() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(1);
            *a.get_mut(i) = 100;
            assert_eq!(*a.get(i), 100);
        }
    }

    // ── Arena::dealloc ──────────────────────────────────────────────────
    mod dealloc {
        use super::*;

        #[test]
        fn returns_the_stored_value() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(55);
            assert_eq!(a.dealloc(i), 55);
        }

        #[test]
        fn decreases_count_by_one() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(1);
            a.dealloc(i);
            assert_eq!(a.count(), 0);
        }
    }

    // ── Arena::is_occupied ──────────────────────────────────────────────
    mod is_occupied {
        use super::*;

        #[test]
        fn is_true_after_alloc() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(7);
            assert!(a.is_occupied(i));
        }

        #[test]
        fn is_false_after_dealloc() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(7);
            a.dealloc(i);
            assert!(!a.is_occupied(i));
        }

        #[test]
        fn is_false_for_out_of_range_index() {
            let a: Arena<u32> = Arena::new();
            assert!(!a.is_occupied(0));
        }
    }

    // ── Arena::iter_occupied ─────────────────────────────────────────────
    mod iter_occupied {
        use super::*;

        #[test]
        fn yields_all_allocated_slots() {
            let mut a: Arena<u32> = Arena::new();
            let i0 = a.alloc(10);
            let i1 = a.alloc(20);
            let i2 = a.alloc(30);
            let items: Vec<(usize, &u32)> = a.iter_occupied().collect();
            assert_eq!(items.len(), 3);
            assert!(items.contains(&(i0, &10)));
            assert!(items.contains(&(i1, &20)));
            assert!(items.contains(&(i2, &30)));
        }

        #[test]
        fn skips_deallocated_slots() {
            let mut a: Arena<u32> = Arena::new();
            let i0 = a.alloc(10);
            let i1 = a.alloc(20);
            a.dealloc(i0);
            let items: Vec<(usize, &u32)> = a.iter_occupied().collect();
            assert_eq!(items.len(), 1);
            assert!(items.contains(&(i1, &20)));
        }

        #[test]
        fn yields_nothing_for_empty_arena() {
            let a: Arena<u32> = Arena::new();
            assert_eq!(a.iter_occupied().count(), 0);
        }
    }

    // ── Arena::default ──────────────────────────────────────────────────
    mod default {
        use super::*;

        #[test]
        fn starts_empty_via_default_trait() {
            let a: Arena<u32> = Arena::default();
            assert_eq!(a.count(), 0);
        }
    }

    // ── Arena::fmt ──────────────────────────────────────────────────────
    mod fmt {
        use super::*;

        #[test]
        fn produces_a_non_empty_debug_string() {
            let mut a: Arena<u32> = Arena::new();
            a.alloc(1);
            let s = format!("{a:?}");
            assert!(s.contains("Arena"), "expected 'Arena' in debug output: {s}");
        }
    }

    // ── Arena::clone ────────────────────────────────────────────────────
    mod clone {
        use super::*;

        #[test]
        fn clone_has_same_count() {
            let mut a: Arena<u32> = Arena::new();
            a.alloc(10);
            a.alloc(20);
            let b = a.clone();
            assert_eq!(b.count(), a.count());
        }

        #[test]
        fn clone_is_independent() {
            let mut a: Arena<u32> = Arena::new();
            let i = a.alloc(1);
            let mut b = a.clone();
            *b.get_mut(i) = 99;
            assert_eq!(*a.get(i), 1, "original must be unaffected");
        }
    }
}
