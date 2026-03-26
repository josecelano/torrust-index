use crate::traits::{Accumulator, Coordinate, Weighable};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Layer<C: Coordinate, V: Accumulator> {
    pub transitions: Vec<Transition<C, V>>,

    pub terminals: Vec<Terminal<C, V>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transition<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub baseline: V,

    pub total: V,

    pub refinement: V,

    pub depth: u32,

    pub v_depth: u32,
}

impl<C: Coordinate, V: Accumulator + Weighable> Transition<C, V> {
    #[must_use]
    pub fn snr(&self) -> Option<f64> {
        let b = self.baseline.weight();
        if b == 0.0 {
            return None;
        }
        Some(self.refinement.weight() / b)
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Terminal<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,

    pub v_depth: u32,
}

impl<C: Coordinate, V: Accumulator> Terminal<C, V> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}
