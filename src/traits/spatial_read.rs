use super::{Accumulator, Coordinate};

pub trait SpatialRead {

    type Coord: Coordinate;

    type Accum: Accumulator;

    #[allow(clippy::type_complexity)]
    fn plateaus(
        &self,
    ) -> std::borrow::Cow<
        '_,
        std::collections::BTreeMap<crate::plateau::BasisEdge<Self::Coord>, crate::plateau::Plateau<Self::Coord, Self::Accum>>,
    >;

    fn get(&self, coord: Self::Coord) -> crate::view::Cell<Self::Coord, Self::Accum>;
}
