use super::{Rng, SpatialRead};

pub trait WeightedSampler: SpatialRead {

    fn sample(&self, rng: &mut impl Rng) -> Option<crate::spatial::view::Cell<Self::Coord, Self::Accum>>;
}
