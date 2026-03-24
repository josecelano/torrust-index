use super::{Rng, SpatialRead};

pub trait WeightedSampler: SpatialRead {

    fn sample(&self, rng: &mut impl Rng) -> Option<crate::view::Cell<Self::Coord, Self::Accum>>;
}
