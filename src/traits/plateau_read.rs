use super::SpatialRead;
use crate::spatial::plateau::{BasisEdge, Plateau};

pub trait PlateauRead: SpatialRead {
    #[allow(clippy::type_complexity)]
    fn plateaus(
        &self,
    ) -> impl Iterator<Item = (&BasisEdge<Self::Coord>, &Plateau<Self::Coord, Self::Accum>)>;
}
