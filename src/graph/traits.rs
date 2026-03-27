use crate::graph::GvGraph;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> crate::traits::SpatialRead
    for GvGraph<C, V, N>
{
    type Coord = C;
    type Accum = V;

    fn get(&self, coord: C) -> crate::spatial::view::Cell<C, V> {
        self.get(coord)
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> crate::traits::PlateauRead
    for GvGraph<C, V, N>
{
    fn plateaus(
        &self,
    ) -> impl Iterator<
        Item = (
            &crate::spatial::plateau::BasisEdge<C>,
            &crate::spatial::plateau::Plateau<C, V>,
        ),
    > {
        self.plateaus.iter()
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

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph};
    #[cfg(feature = "dynamic-contour-tracking")]
    use crate::traits::PlateauRead;
    use crate::traits::{SpatialRead, SpatialWrite, TemporalDecay, WeightedSampler};

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            depth_create: 3,
            depth_evict: 5,
            budget: None,
            alpha_relax: 0.5,
            bounded_eviction: false,
        }
    }

    type TestGraph = GvGraph<u8, u32, 8>;

    // ── SpatialRead impl ─────────────────────────────────────────────────
    mod spatial_read {
        use super::*;

        fn call_get_via_trait<T: SpatialRead<Coord = u8, Accum = u32>>(r: &T, coord: u8) -> u32 {
            r.get(coord).intensity
        }

        #[test]
        fn get_returns_zero_for_unobserved_coordinate() {
            let g: TestGraph = GvGraph::new(make_config());
            assert_eq!(call_get_via_trait(&g, 42u8), 0u32);
        }
    }

    // ── PlateauRead impl ─────────────────────────────────────────────────
    #[cfg(feature = "dynamic-contour-tracking")]
    mod plateau_read {
        use super::*;

        fn call_plateaus_via_trait<T: PlateauRead<Coord = u8, Accum = u32>>(r: &T) -> usize {
            r.plateaus().count()
        }

        #[test]
        fn plateaus_returns_at_least_one_for_fresh_graph() {
            let g: TestGraph = GvGraph::new(make_config());
            assert!(call_plateaus_via_trait(&g) >= 1);
        }
    }

    // ── SpatialWrite impl ────────────────────────────────────────────────
    mod spatial_write {
        use super::*;

        fn observe_via_trait<T: SpatialWrite<Coord = u8, Accum = u32>>(
            w: &mut T,
            coord: u8,
            delta: u32,
        ) {
            w.observe(coord, delta);
        }

        #[test]
        fn observe_increases_total_sum() {
            let mut g: TestGraph = GvGraph::new(make_config());
            observe_via_trait(&mut g, 0u8, 10u32);
            assert_eq!(g.total_sum(), 10u32);
        }
    }

    // ── TemporalDecay impl ───────────────────────────────────────────────
    mod temporal_decay {
        use super::*;

        fn decay_via_trait<T: TemporalDecay<Coord = u8, Accum = u32>>(
            d: &mut T,
            root: crate::handle::GNodeId,
        ) {
            d.decay(root, 0.5, 0.0);
        }

        #[test]
        fn decay_does_not_panic_on_fresh_graph() {
            let mut g: TestGraph = GvGraph::new(make_config());
            let root = g.g_root();
            decay_via_trait(&mut g, root);
        }
    }

    // ── WeightedSampler impl ─────────────────────────────────────────────
    mod weighted_sampler {
        use super::*;
        use crate::traits::Rng;

        struct ConstantRng(f64);
        impl Rng for ConstantRng {
            fn next_f64(&mut self) -> f64 {
                self.0
            }
        }

        fn sample_via_trait<T: WeightedSampler<Coord = u8, Accum = u32>>(
            s: &T,
            rng: &mut impl Rng,
        ) -> Option<u8> {
            s.sample(rng).map(|c| c.start)
        }

        #[test]
        fn sample_returns_none_for_zero_sum_graph() {
            let g: TestGraph = GvGraph::new(make_config());
            let mut rng = ConstantRng(0.5);
            assert!(sample_via_trait(&g, &mut rng).is_none());
        }

        #[test]
        fn sample_returns_some_after_observation() {
            let mut g: TestGraph = GvGraph::new(make_config());
            g.observe(64u8, 100u32);
            let mut rng = ConstantRng(0.5);
            let result = sample_via_trait(&g, &mut rng);
            assert!(result.is_some());
        }
    }
}
