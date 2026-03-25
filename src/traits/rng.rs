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

#[cfg(test)]
mod tests {
    // ── Rng::next_f64 ───────────────────────────────────────────────────────
    mod next_f64 {
        use crate::traits::Rng;

        struct ConstantRng(f64);

        impl Rng for ConstantRng {
            fn next_f64(&mut self) -> f64 {
                self.0
            }
        }

        #[test]
        fn returns_the_value_provided_by_the_implementation() {
            let mut rng = ConstantRng(0.42);
            assert!((rng.next_f64() - 0.42_f64).abs() < f64::EPSILON);
        }

        #[test]
        fn can_be_called_multiple_times() {
            let mut rng = ConstantRng(1.0);
            assert!((rng.next_f64() - 1.0_f64).abs() < f64::EPSILON);
            assert!((rng.next_f64() - 1.0_f64).abs() < f64::EPSILON);
        }
    }

    #[cfg(feature = "rand")]
    mod rand_core_backed {
        use crate::traits::Rng;
        use std::convert::Infallible;

        struct FixedRng(u64);

        // rand_core 0.10: TryRng is the base trait; a blanket
        // `impl<R: TryRng<Error = Infallible>> Rng for R` gives rand_core::Rng.
        impl rand_core::TryRng for FixedRng {
            type Error = Infallible;

            fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
                Ok((self.0 >> 32) as u32)
            }

            fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
                Ok(self.0)
            }

            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
                for (i, d) in dest.iter_mut().enumerate() {
                    *d = self.0.to_le_bytes()[i % 8];
                }
                Ok(())
            }
        }

        #[test]
        fn rand_core_rng_impl_yields_value_in_unit_interval() {
            // FixedRng: TryRng<Error=Infallible> → rand_core::Rng (blanket)
            // → crate::traits::Rng (blanket, #[cfg(feature = "rand")]).
            let mut rng = FixedRng(1u64 << 53);
            let v = rng.next_f64();
            assert!(v >= 0.0 && v < 1.0, "expected value in [0,1), got {v}");
        }

        #[test]
        fn try_next_u32_returns_high_word_of_stored_value() {
            // next_u32() on a rand_core::Rng delegates to try_next_u32().
            use rand_core::Rng;
            let mut rng = FixedRng(0xFFFF_FFFF_0000_0001u64);
            // try_next_u32 returns Ok((self.0 >> 32) as u32) = 0xFFFF_FFFF
            assert_eq!(rng.next_u32(), 0xFFFF_FFFFu32);
        }

        #[test]
        fn try_fill_bytes_fills_buffer_from_le_bytes_of_stored_value() {
            // fill_bytes() delegates to try_fill_bytes().
            use rand_core::Rng;
            let val = 0x0102_0304_0506_0708u64;
            let mut rng = FixedRng(val);
            let mut buf = [0u8; 4];
            rng.fill_bytes(&mut buf);
            // to_le_bytes: least-significant byte first
            let le = val.to_le_bytes();
            assert_eq!(&buf, &le[..4]);
        }
    }
}
