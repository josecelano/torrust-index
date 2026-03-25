use std::fmt::Debug;

pub trait Accumulator: Copy + PartialOrd + Debug + Default + Send + Sync + 'static {
    fn zero() -> Self;

    #[must_use]
    fn add(self, other: Self) -> Self;

    #[must_use]
    fn sub(self, other: Self) -> Self;
}

macro_rules! impl_accumulator_uint {
    ($($ty:ty),+) => {$(
        impl Accumulator for $ty {
            #[inline]
            fn zero() -> Self { 0 }

            #[inline]
            fn add(self, other: Self) -> Self {
                self.checked_add(other).expect("attempt to add with overflow")
            }

            #[inline]
            fn sub(self, other: Self) -> Self {
                self.checked_sub(other).expect("attempt to subtract with overflow")
            }
        }
    )+};
}

impl_accumulator_uint!(u8, u16, u32, u64, u128);

impl Accumulator for f32 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn add(self, other: Self) -> Self {
        self + other
    }

    #[inline]
    fn sub(self, other: Self) -> Self {
        self - other
    }
}

impl Accumulator for f64 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn add(self, other: Self) -> Self {
        self + other
    }

    #[inline]
    fn sub(self, other: Self) -> Self {
        self - other
    }
}

#[cfg(test)]
mod tests {
    // ── Accumulator::zero ───────────────────────────────────────────────────
    mod zero {
        use crate::traits::Accumulator;

        #[test]
        fn returns_zero_for_unsigned_integers() {
            assert_eq!(u32::zero(), 0u32);
            assert_eq!(u64::zero(), 0u64);
        }

        #[test]
        fn returns_zero_for_f32() {
            assert_eq!(f32::zero(), 0.0_f32);
        }

        #[test]
        fn returns_zero_for_f64() {
            assert_eq!(f64::zero(), 0.0_f64);
        }
    }

    // ── Accumulator::add ────────────────────────────────────────────────────
    mod add {
        use crate::traits::Accumulator;
        use rstest::rstest;

        #[rstest]
        #[case(0u32, 0u32, 0u32)]
        #[case(3u32, 4u32, 7u32)]
        #[case(u32::MAX - 1, 1u32, u32::MAX)]
        fn sums_two_u32_values(#[case] a: u32, #[case] b: u32, #[case] expected: u32) {
            assert_eq!(u32::add(a, b), expected);
        }

        #[test]
        fn with_zero_is_identity_for_u32() {
            assert_eq!(u32::add(u32::zero(), 42), 42u32);
        }

        #[test]
        fn is_commutative_for_u32() {
            assert_eq!(u32::add(5, 3), u32::add(3, 5));
        }

        #[test]
        fn sums_two_f32_values() {
            assert!((f32::add(1.5, 2.5) - 4.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn sums_two_f64_values() {
            let a = 3.14_f64;
            let b = 2.71_f64;
            assert!((f64::add(a, b) - (a + b)).abs() < f64::EPSILON);
        }

        #[test]
        #[should_panic(expected = "attempt to add with overflow")]
        fn panics_on_u32_overflow() {
            let _ = u32::add(u32::MAX, 1);
        }
    }

    // ── Accumulator::sub ────────────────────────────────────────────────────
    mod sub {
        use crate::traits::Accumulator;
        use rstest::rstest;

        #[rstest]
        #[case(10u32, 3u32, 7u32)]
        #[case(0u32, 0u32, 0u32)]
        #[case(u32::MAX, u32::MAX, 0u32)]
        fn returns_the_difference_for_u32(#[case] a: u32, #[case] b: u32, #[case] expected: u32) {
            assert_eq!(u32::sub(a, b), expected);
        }

        #[test]
        fn returns_the_difference_for_f32() {
            assert!((f32::sub(5.0, 2.0) - 3.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn with_add_forms_a_round_trip_for_u64() {
            let start = 1_000_000u64;
            let delta = 42u64;
            assert_eq!(u64::sub(u64::add(start, delta), delta), start);
        }

        #[test]
        fn with_add_forms_a_round_trip_for_f64() {
            let a = 3.14_f64;
            let b = 2.71_f64;
            assert!((f64::sub(f64::add(a, b), b) - a).abs() < 1e-10);
        }

        #[test]
        #[should_panic(expected = "attempt to subtract with overflow")]
        fn panics_on_u32_underflow() {
            let _ = u32::sub(0, 1);
        }
    }
}
