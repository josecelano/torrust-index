use std::fmt::Debug;

pub trait Coordinate: Copy + PartialOrd + Debug + Default + Send + Sync + 'static {
    const BITS: u32;

    fn zero() -> Self;

    fn domain_max(n: u32) -> Self;

    fn midpoint(a: Self, b: Self) -> Self;

    fn width(start: Self, end: Self) -> Self;

    fn is_final(start: Self, end: Self, depth: u32, n: u32) -> bool;

    fn from_u64(v: u64) -> Self;

    fn to_f64(self) -> f64;

    fn is_nan(self) -> bool;

    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering;
}

macro_rules! impl_coordinate_uint {
    ($($ty:ty),+) => {$(
        impl Coordinate for $ty {
            const BITS: u32 = <$ty>::BITS;

            #[inline]
            fn zero() -> Self { 0 }

            #[inline]
            fn domain_max(n: u32) -> Self {
                if n == Self::BITS {
                    <$ty>::MAX
                } else {
                    1 << n
                }
            }

            #[inline]
            fn midpoint(a: Self, b: Self) -> Self {
                a + (b - a) / 2
            }

            #[inline]
            fn width(start: Self, end: Self) -> Self {
                end - start
            }

            #[inline]
            fn is_final(start: Self, end: Self, _depth: u32, _n: u32) -> bool {
                end - start == 1
            }

            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
            fn from_u64(v: u64) -> Self {
                v as Self
            }

            #[inline]
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
            fn to_f64(self) -> f64 {
                self as f64
            }

            #[inline]
            fn is_nan(self) -> bool {
                false
            }

            #[inline]
            fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
                Ord::cmp(self, other)
            }
        }
    )+};
}

impl_coordinate_uint!(u8, u16, u32, u64, u128);

/// Subtrait of [`Coordinate`] for integer (discrete) coordinate types.
///
/// Integer coordinates support a `next_value` operation (increment by one
/// unit) that has no meaningful equivalent for float coordinates. Bounding
/// a function on `DiscreteCoordinate` instead of `Coordinate` makes this
/// requirement visible at compile time and prevents accidental use with
/// floating-point coordinate types.
pub trait DiscreteCoordinate: Coordinate {
    /// Returns the smallest coordinate strictly greater than `self`.
    #[must_use]
    fn next_value(self) -> Self;
}

macro_rules! impl_discrete_coordinate_uint {
    ($($ty:ty),+) => {$(
        impl DiscreteCoordinate for $ty {
            #[inline]
            fn next_value(self) -> Self {
                self + 1
            }
        }
    )+};
}

impl_discrete_coordinate_uint!(u8, u16, u32, u64, u128);

impl Coordinate for f32 {
    const BITS: u32 = 32;

    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    fn domain_max(n: u32) -> Self {
        2.0_f32.powi(n as i32)
    }

    #[inline]
    fn midpoint(a: Self, b: Self) -> Self {
        a + (b - a) / 2.0
    }

    #[inline]
    fn width(start: Self, end: Self) -> Self {
        end - start
    }

    #[inline]
    fn is_final(_start: Self, _end: Self, depth: u32, n: u32) -> bool {
        depth >= n
    }

    #[inline]
    #[allow(clippy::cast_precision_loss)]
    fn from_u64(v: u64) -> Self {
        v as Self
    }

    #[inline]
    fn to_f64(self) -> f64 {
        f64::from(self)
    }

    #[inline]
    fn is_nan(self) -> bool {
        self.is_nan()
    }

    #[inline]
    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
        Self::total_cmp(self, other)
    }
}

impl Coordinate for f64 {
    const BITS: u32 = 64;

    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    fn domain_max(n: u32) -> Self {
        2.0_f64.powi(n as i32)
    }

    #[inline]
    fn midpoint(a: Self, b: Self) -> Self {
        a + (b - a) / 2.0
    }

    #[inline]
    fn width(start: Self, end: Self) -> Self {
        end - start
    }

    #[inline]
    fn is_final(_start: Self, _end: Self, depth: u32, n: u32) -> bool {
        depth >= n
    }

    #[inline]
    #[allow(clippy::cast_precision_loss)]
    fn from_u64(v: u64) -> Self {
        v as Self
    }

    #[inline]
    fn to_f64(self) -> f64 {
        self
    }

    #[inline]
    fn is_nan(self) -> bool {
        self.is_nan()
    }

    #[inline]
    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
        Self::total_cmp(self, other)
    }
}

#[cfg(test)]
mod tests {
    // ── Coordinate::zero ────────────────────────────────────────────────────
    mod zero {
        use crate::traits::Coordinate;

        #[test]
        fn returns_zero_for_unsigned_integers() {
            assert_eq!(u8::zero(), 0u8);
            assert_eq!(u32::zero(), 0u32);
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

    // ── Coordinate::domain_max ──────────────────────────────────────────────
    mod domain_max {
        use crate::traits::Coordinate;
        use rstest::rstest;

        #[test]
        fn returns_type_max_when_n_equals_all_bits_for_u8() {
            assert_eq!(u8::domain_max(8), u8::MAX);
        }

        #[test]
        fn returns_type_max_when_n_equals_all_bits_for_u32() {
            assert_eq!(u32::domain_max(32), u32::MAX);
        }

        #[rstest]
        #[case(1, 2u8)] // 2^1
        #[case(2, 4u8)] // 2^2
        #[case(3, 8u8)] // 2^3
        #[case(4, 16u8)] // 2^4
        #[case(7, 128u8)] // 2^7
        fn returns_a_power_of_two_for_each_u8_bit_depth(#[case] n: u32, #[case] expected: u8) {
            assert_eq!(u8::domain_max(n), expected);
        }

        #[test]
        fn returns_a_power_of_two_for_f32() {
            assert!((f32::domain_max(4) - 16.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn returns_a_power_of_two_for_f64() {
            assert!((f64::domain_max(4) - 16.0_f64).abs() < f64::EPSILON);
        }
    }

    // ── Coordinate::midpoint ────────────────────────────────────────────────
    mod midpoint {
        use crate::traits::Coordinate;

        #[test]
        fn bisects_an_even_integer_interval() {
            assert_eq!(<u8 as Coordinate>::midpoint(0, 128), 64u8);
        }

        #[test]
        fn rounds_toward_lower_bound_on_odd_width() {
            // midpoint(0, 3) == 0 + (3 - 0) / 2 == 1.
            assert_eq!(<u8 as Coordinate>::midpoint(0, 3), 1u8);
        }

        #[test]
        fn returns_the_exact_centre_of_a_large_u32_interval() {
            assert_eq!(<u32 as Coordinate>::midpoint(0, 1_000_000), 500_000u32);
        }

        #[test]
        fn bisects_a_unit_f32_interval() {
            assert!((<f32 as Coordinate>::midpoint(0.0, 1.0) - 0.5_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn bisects_a_unit_f64_interval() {
            assert!((<f64 as Coordinate>::midpoint(0.0, 1.0) - 0.5_f64).abs() < f64::EPSILON);
        }
    }

    // ── Coordinate::width ───────────────────────────────────────────────────
    mod width {
        use crate::traits::Coordinate;

        #[test]
        fn returns_the_span_between_endpoints_for_integers() {
            assert_eq!(u8::width(10, 50), 40u8);
        }

        #[test]
        fn returns_the_span_between_endpoints_for_f32() {
            assert!((f32::width(1.0, 4.0) - 3.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn returns_the_span_between_endpoints_for_f64() {
            assert!((f64::width(1.0, 4.0) - 3.0_f64).abs() < f64::EPSILON);
        }
    }

    // ── Coordinate::is_final ────────────────────────────────────────────────
    mod is_final {
        use crate::traits::Coordinate;
        use rstest::rstest;

        // Integer types: final when the gap between start and end is exactly 1.
        #[rstest]
        #[case(5u8, 6u8, true)] // gap of 1 → final
        #[case(0u8, 1u8, true)] // minimum gap → final
        #[case(0u8, 4u8, false)] // gap of 4 → not final
        #[case(10u8, 12u8, false)] // gap of 2 → not final
        fn integer_is_final_iff_gap_equals_one(
            #[case] start: u8,
            #[case] end: u8,
            #[case] expected: bool,
        ) {
            assert_eq!(u8::is_final(start, end, 0, 8), expected);
        }

        // Floating-point types: final when depth reaches n.
        #[rstest]
        #[case(8u32, 8u32, true)] // depth == n → final
        #[case(9u32, 8u32, true)] // depth > n → also final
        #[case(7u32, 8u32, false)] // depth < n → not final
        #[case(0u32, 8u32, false)] // depth 0 < n=8 → not final
        fn float_is_final_iff_depth_reaches_n(
            #[case] depth: u32,
            #[case] n: u32,
            #[case] expected: bool,
        ) {
            assert_eq!(f32::is_final(0.0, 0.5, depth, n), expected);
        }

        #[rstest]
        #[case(8u32, 8u32, true)]
        #[case(9u32, 8u32, true)]
        #[case(7u32, 8u32, false)]
        #[case(0u32, 8u32, false)]
        fn f64_is_final_iff_depth_reaches_n(
            #[case] depth: u32,
            #[case] n: u32,
            #[case] expected: bool,
        ) {
            assert_eq!(f64::is_final(0.0_f64, 0.5_f64, depth, n), expected);
        }
    }

    // ── Coordinate::from_u64 ────────────────────────────────────────────────
    mod from_u64 {
        use crate::traits::Coordinate;

        #[test]
        fn truncates_to_the_target_integer_type() {
            assert_eq!(u8::from_u64(256 + 7), 7u8);
        }

        #[test]
        fn converts_to_f32() {
            assert!((f32::from_u64(100) - 100.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn converts_to_f64() {
            assert!((f64::from_u64(100) - 100.0_f64).abs() < f64::EPSILON);
        }
    }

    // ── DiscreteCoordinate::next_value ─────────────────────────────────────
    mod next_value {
        use crate::traits::DiscreteCoordinate;

        #[test]
        fn increments_an_integer_by_one() {
            assert_eq!(5u8.next_value(), 6u8);
        }
    }

    // ── Coordinate::to_f64 ─────────────────────────────────────────────────
    mod to_f64 {
        use crate::traits::Coordinate;

        #[test]
        fn produces_an_exact_f64_for_small_u8_values() {
            assert!((200u8.to_f64() - 200.0_f64).abs() < f64::EPSILON);
        }

        #[test]
        fn promotes_f32_to_f64_without_precision_loss() {
            let v = 3.14_f32;
            assert!((v.to_f64() - f64::from(v)).abs() < 1e-6);
        }

        #[test]
        fn is_identity_for_f64() {
            let v = 42.5_f64;
            assert!((v.to_f64() - v).abs() < f64::EPSILON);
        }
    }

    // ── Coordinate::is_nan ──────────────────────────────────────────────────
    mod is_nan {
        use crate::traits::Coordinate;

        #[test]
        fn always_returns_false_for_integer_types() {
            assert!(!42u8.is_nan());
        }

        #[test]
        fn returns_true_for_f32_nan() {
            // Explicit UFCS — f32::NAN.is_nan() would call the *inherent* std method,
            // never dispatching through the Coordinate trait impl.
            assert!(<f32 as Coordinate>::is_nan(f32::NAN));
        }

        #[test]
        fn returns_false_for_a_normal_f32_value() {
            assert!(!<f32 as Coordinate>::is_nan(1.0_f32));
        }

        #[test]
        fn returns_true_for_f64_nan() {
            assert!(<f64 as Coordinate>::is_nan(f64::NAN));
        }

        #[test]
        fn returns_false_for_a_normal_f64_value() {
            assert!(!<f64 as Coordinate>::is_nan(1.0_f64));
        }
    }

    // ── Coordinate::total_cmp ───────────────────────────────────────────────
    mod total_cmp {
        use crate::traits::Coordinate;
        use rstest::rstest;
        use std::cmp::Ordering;

        #[rstest]
        #[case(1u8, 2u8, Ordering::Less)]
        #[case(2u8, 2u8, Ordering::Equal)]
        #[case(3u8, 2u8, Ordering::Greater)]
        fn integer_ordering_matches_numeric_order(
            #[case] a: u8,
            #[case] b: u8,
            #[case] expected: Ordering,
        ) {
            assert_eq!(a.total_cmp(&b), expected);
        }

        #[test]
        fn produces_less_for_f32_values_in_ascending_order() {
            assert_eq!(1.0_f32.total_cmp(&2.0_f32), Ordering::Less);
        }

        #[test]
        fn f32_equal_and_greater_cases() {
            assert_eq!(2.0_f32.total_cmp(&2.0_f32), Ordering::Equal);
            assert_eq!(3.0_f32.total_cmp(&2.0_f32), Ordering::Greater);
        }

        #[rstest]
        #[case(1.0_f64, 2.0_f64, Ordering::Less)]
        #[case(2.0_f64, 2.0_f64, Ordering::Equal)]
        #[case(3.0_f64, 2.0_f64, Ordering::Greater)]
        fn f64_ordering_is_correct(#[case] a: f64, #[case] b: f64, #[case] expected: Ordering) {
            assert_eq!(a.total_cmp(&b), expected);
        }

        // The tests above call f32/f64's inherent total_cmp, which bypasses the
        // Coordinate trait impl. The following tests use explicit UFCS to dispatch
        // through the <f32 as Coordinate>::total_cmp and <f64 as Coordinate>::total_cmp
        // implementations, exercising those method bodies.
        #[test]
        fn f32_dispatched_through_coordinate_trait_total_cmp() {
            assert_eq!(
                <f32 as Coordinate>::total_cmp(&1.0_f32, &2.0_f32),
                Ordering::Less
            );
        }

        #[test]
        fn f64_dispatched_through_coordinate_trait_total_cmp() {
            assert_eq!(
                <f64 as Coordinate>::total_cmp(&1.0_f64, &2.0_f64),
                Ordering::Less
            );
        }
    }
}
