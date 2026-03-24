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
