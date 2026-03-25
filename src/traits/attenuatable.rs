use super::Accumulator;

pub trait Attenuatable: Accumulator {
    #[must_use]
    fn attenuate(self, factor: f64) -> Self;
}

macro_rules! impl_attenuatable_uint {
    ($($ty:ty),+) => {$(
        impl Attenuatable for $ty {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn attenuate(self, factor: f64) -> Self {
                (self as f64 * factor) as Self
            }
        }
    )+};
}

impl_attenuatable_uint!(u8, u16, u32, u64, u128);

impl Attenuatable for f32 {
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn attenuate(self, factor: f64) -> Self {
        if self == 0.0 || factor == 0.0 {
            return 0.0;
        }
        (f64::from(self) * factor) as Self
    }
}

impl Attenuatable for f64 {
    #[inline]
    fn attenuate(self, factor: f64) -> Self {
        if self == 0.0 || factor == 0.0 {
            return 0.0;
        }
        self * factor
    }
}

#[cfg(test)]
mod tests {
    // ── Attenuatable::attenuate ─────────────────────────────────────────────
    mod attenuate {
        use crate::traits::Attenuatable;
        use rstest::rstest;

        #[rstest]
        #[case(100u32, 0.5_f64, 50u32)]    // factor 0.5 halves the value
        #[case(200u32, 1.0_f64, 200u32)]   // factor 1.0 is identity
        #[case(999u32, 0.0_f64, 0u32)]     // factor 0.0 drives to zero
        #[case(3u32, 0.4_f64, 1u32)]       // 3 × 0.4 = 1.2, truncated to 1
        #[case(1000u32, 0.75_f64, 750u32)] // three-quarter scale
        fn u32_attenuation(#[case] value: u32, #[case] factor: f64, #[case] expected: u32) {
            assert_eq!(value.attenuate(factor), expected);
        }

        #[test]
        fn scales_a_u64_value_by_three_quarters() {
            assert_eq!(1000u64.attenuate(0.75), 750u64);
        }

        // f32 and f64 use approximate comparisons, so kept as individual tests.
        #[test]
        fn halves_an_f32_value_when_factor_is_one_half() {
            assert!((10.0_f32.attenuate(0.5) - 5.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn returns_zero_for_f32_when_value_is_zero() {
            assert_eq!(0.0_f32.attenuate(0.9), 0.0_f32);
        }

        #[test]
        fn returns_zero_for_f32_when_factor_is_zero() {
            assert_eq!(100.0_f32.attenuate(0.0), 0.0_f32);
        }

        #[test]
        fn halves_an_f64_value_when_factor_is_one_half() {
            assert!((8.0_f64.attenuate(0.5) - 4.0_f64).abs() < f64::EPSILON);
        }

        #[test]
        fn returns_zero_for_f64_when_value_is_zero() {
            assert_eq!(0.0_f64.attenuate(0.5), 0.0_f64);
        }

        #[test]
        fn returns_zero_for_f64_when_factor_is_zero() {
            assert_eq!(42.0_f64.attenuate(0.0), 0.0_f64);
        }
    }
}
