use super::{Observation, SpatialRead};

pub trait SpatialWrite: SpatialRead {

    fn observe<O: Observation<Self::Accum>>(&mut self, coord: Self::Coord, delta: O);
}
