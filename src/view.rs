use crate::nodes::gnode::GState;
use crate::handle::GNodeId;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub own: V,

    pub sum: V,

    pub depth: u32,

    pub state: GState,

    pub gnode_id: GNodeId,

    pub parent: Option<GNodeId>,
}

impl<C: Coordinate, V: Accumulator> Node<C, V> {
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

    #[inline]
    #[must_use]
    pub fn to_cell(self) -> Option<Cell<C, V>> {
        if self.state == GState::Terminal {
            Some(Cell {
                start: self.start,
                end: self.end,
                intensity: self.own,
                depth: self.depth,
            })
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.state == GState::Terminal
    }

    #[inline]
    #[must_use]
    pub fn refinement(&self) -> V {
        V::sub(self.sum, self.own)
    }

    #[inline]
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}
