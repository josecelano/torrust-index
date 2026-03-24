use crate::graph::GvGraph;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> crate::traits::SpatialRead
    for GvGraph<C, V, N>
{
    type Coord = C;
    type Accum = V;

    fn plateaus(
        &self,
    ) -> std::borrow::Cow<
        '_,
        std::collections::BTreeMap<
            crate::spatial::plateau::BasisEdge<C>,
            crate::spatial::plateau::Plateau<C, V>,
        >,
    > {
        self.plateaus()
    }

    fn get(&self, coord: C) -> crate::spatial::view::Cell<C, V> {
        self.get(coord)
    }
}

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> crate::traits::SpatialWrite
    for GvGraph<C, V, N>
{
    fn observe<O: crate::traits::Observation<V>>(&mut self, coord: C, delta: O) {
        self.observe(coord, delta);
    }
}

impl<C: Coordinate, V: Accumulator + crate::traits::Attenuatable + Inspectable, const N: u32>
    crate::traits::TemporalDecay for GvGraph<C, V, N>
{
    fn decay(&mut self, root: crate::handle::GNodeId, attenuation: f64, q: f64) {
        self.decay(root, attenuation, q);
    }
}

impl<C: Coordinate, V: Accumulator + Inspectable + crate::traits::Weighable, const N: u32>
    crate::traits::WeightedSampler for GvGraph<C, V, N>
{
    fn sample(
        &self,
        rng: &mut impl crate::traits::Rng,
    ) -> Option<crate::spatial::view::Cell<C, V>> {
        self.sample(rng)
    }
}
