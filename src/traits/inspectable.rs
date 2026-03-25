use super::Accumulator;

pub trait Inspectable: Accumulator {
    fn to_f64_approx(self) -> f64;

    fn from_f64(v: f64) -> Self;
}

macro_rules! impl_inspectable_uint {
    ($($ty:ty),+) => {$(
        impl Inspectable for $ty {
            #[inline]
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
            fn to_f64_approx(self) -> f64 {
                self as f64
            }

            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless)]
            fn from_f64(v: f64) -> Self {
                v as Self
            }
        }
    )+};
}

impl_inspectable_uint!(u8, u16, u32, u64, u128);

impl Inspectable for f32 {
    #[inline]
    fn to_f64_approx(self) -> f64 {
        f64::from(self)
    }

    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn from_f64(v: f64) -> Self {
        v as Self
    }
}

impl Inspectable for f64 {
    #[inline]
    fn to_f64_approx(self) -> f64 {
        self
    }

    #[inline]
    fn from_f64(v: f64) -> Self {
        v
    }
}

#[cfg(test)]
mod tests {
    // ── Inspectable::to_f64_approx ──────────────────────────────────────────
    mod to_f64_approx {
        use crate::traits::Inspectable;

        #[test]
        fn converts_u32_to_exact_f64() {
            assert!((42u32.to_f64_approx() - 42.0_f64).abs() < f64::EPSILON);
        }

        #[test]
        fn converts_zero_u32_to_zero_f64() {
            assert_eq!(0u32.to_f64_approx(), 0.0_f64);
        }

        #[test]
        fn promotes_f32_to_f64_without_precision_loss() {
            let v = 1.5_f32;
            assert!((v.to_f64_approx() - f64::from(v)).abs() < f64::EPSILON);
        }

        #[test]
        fn is_identity_for_f64() {
            let v = 3.14_f64;
            assert!((v.to_f64_approx() - v).abs() < f64::EPSILON);
        }
    }

    // ── Inspectable::from_f64 ─────────────────────────────────────────────
    mod from_f64 {
        use crate::traits::Inspectable;
        use rstest::rstest;

        #[rstest]
        #[case(0.0_f64, 0u32)]
        #[case(100.0_f64, 100u32)]
        #[case(42.9_f64, 42u32)] // truncates toward zero
        fn converts_f64_to_u32(#[case] input: f64, #[case] expected: u32) {
            assert_eq!(u32::from_f64(input), expected);
        }

        #[test]
        fn is_identity_for_f64() {
            let v = 2.718_f64;
            assert!((f64::from_f64(v) - v).abs() < f64::EPSILON);
        }

        #[test]
        fn converts_f64_to_f32_with_expected_precision() {
            assert!((f32::from_f64(1.5_f64) - 1.5_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn round_trips_u32_through_f64() {
            let start = 200u32;
            assert_eq!(u32::from_f64(start.to_f64_approx()), start);
        }
    }
}
