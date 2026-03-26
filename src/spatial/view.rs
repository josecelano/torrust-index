use crate::traits::{Accumulator, Coordinate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,
}

impl<C: Coordinate, V: Accumulator> Span<C, V> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cell<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,
}

impl<C: Coordinate, V: Accumulator> Cell<C, V> {
    #[inline]
    #[must_use]
    pub const fn to_span(self) -> Span<C, V> {
        Span {
            start: self.start,
            end: self.end,
            intensity: self.intensity,
            depth: self.depth,
        }
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }

    #[inline]
    #[must_use]
    pub fn is_final(&self, n: u32) -> bool {
        C::is_final(self.start, self.end, self.depth, n)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, Span};
    use rstest::rstest;

    fn span(start: u8, end: u8, intensity: u32, depth: u32) -> Span<u8, u32> {
        Span {
            start,
            end,
            intensity,
            depth,
        }
    }

    fn cell(start: u8, end: u8, intensity: u32, depth: u32) -> Cell<u8, u32> {
        Cell {
            start,
            end,
            intensity,
            depth,
        }
    }

    // ── Span::width ──────────────────────────────────────────────────────
    mod span_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            assert_eq!(span(4, 12, 0, 0).width(), 8u8);
        }

        #[test]
        fn returns_zero_when_start_equals_end() {
            assert_eq!(span(5, 5, 0, 0).width(), 0u8);
        }
    }

    // ── Cell::to_span ────────────────────────────────────────────────────
    mod cell_to_span {
        use super::*;

        #[test]
        fn preserves_all_fields() {
            let c = cell(2, 10, 99, 3);
            let s = c.to_span();
            assert_eq!(s.start, 2u8);
            assert_eq!(s.end, 10u8);
            assert_eq!(s.intensity, 99u32);
            assert_eq!(s.depth, 3u32);
        }
    }

    // ── Cell::width ──────────────────────────────────────────────────────
    mod cell_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            assert_eq!(cell(0, 16, 0, 0).width(), 16u8);
        }
    }

    // ── Cell::is_final ───────────────────────────────────────────────────
    mod cell_is_final {
        use super::*;

        // For u8 coordinates: is_final returns true iff end - start == 1
        #[rstest]
        #[case(0u8, 1u8, 0u32, true)]
        #[case(0u8, 2u8, 0u32, false)]
        #[case(3u8, 4u8, 5u32, true)]
        #[case(0u8, 4u8, 0u32, false)]
        fn matches_coordinate_is_final(
            #[case] start: u8,
            #[case] end: u8,
            #[case] depth: u32,
            #[case] expected: bool,
        ) {
            let c = cell(start, end, 0, depth);
            assert_eq!(c.is_final(8), expected);
        }
    }
}
