use super::{Accumulator, Coordinate};

pub trait SpatialRead {
    type Coord: Coordinate;

    type Accum: Accumulator;

    fn get(&self, coord: Self::Coord) -> crate::spatial::view::Cell<Self::Coord, Self::Accum>;
}
