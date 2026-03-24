use super::{Accumulator, Coordinate};

pub trait SpatialRead {

    type Coord: Coordinate;

    type Accum: Accumulator;

    #[allow(clippy::type_complexity)]
    fn plateaus(
        &self,
    ) -> std::borrow::Cow<
        '_,
        std::collections::BTreeMap<crate::spatial::plateau::BasisEdge<Self::Coord>, crate::spatial::plateau::Plateau<Self::Coord, Self::Accum>>,
    >;

    fn get(&self, coord: Self::Coord) -> crate::spatial::view::Cell<Self::Coord, Self::Accum>;
}
