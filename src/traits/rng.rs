pub trait Rng {
    fn next_f64(&mut self) -> f64;
}

#[cfg(feature = "rand")]
impl<T: rand_core::Rng> Rng for T {
    #[inline]
    #[allow(clippy::cast_precision_loss)]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}
