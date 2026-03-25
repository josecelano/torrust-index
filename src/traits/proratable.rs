use super::Accumulator;

pub trait Proratable: Accumulator {
    #[must_use]
    fn prorate(self, portion: u64, total: u64) -> Self;

    #[must_use]
    fn scale_by(self, ratio: f64) -> Self;
}

macro_rules! impl_proratable_uint {
    ($($ty:ty),+) => {$(
        impl Proratable for $ty {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
            fn prorate(self, portion: u64, total: u64) -> Self {
                if total == 0 { return <Self as Accumulator>::zero(); }
                ((self as u128) * (portion as u128) / (total as u128)) as Self
            }

            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn scale_by(self, ratio: f64) -> Self {
                (self as f64 * ratio) as Self
            }
        }
    )+};
}

impl_proratable_uint!(u8, u16, u32, u64, u128);

impl Proratable for f32 {
    #[inline]
    #[allow(clippy::cast_precision_loss)]
    fn prorate(self, portion: u64, total: u64) -> Self {
        if total == 0 {
            return <Self as Accumulator>::zero();
        }
        self * (portion as Self / total as Self)
    }

    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn scale_by(self, ratio: f64) -> Self {
        (f64::from(self) * ratio) as Self
    }
}

impl Proratable for f64 {
    #[inline]
    #[allow(clippy::cast_precision_loss)]
    fn prorate(self, portion: u64, total: u64) -> Self {
        if total == 0 {
            return <Self as Accumulator>::zero();
        }
        self * (portion as Self / total as Self)
    }

    #[inline]
    fn scale_by(self, ratio: f64) -> Self {
        self * ratio
    }
}

#[cfg(test)]
mod tests {
    // ── Proratable::prorate ─────────────────────────────────────────────────
    mod prorate {
        use crate::traits::Proratable;
        use rstest::rstest;

        #[rstest]
        #[case(100u32, 1, 4, 25u32)] // 100 × ¼
        #[case(100u32, 3, 4, 75u32)] // 100 × ¾
        #[case(100u32, 4, 4, 100u32)] // 100 × 1 (full portion)
        #[case(100u32, 0, 4, 0u32)] // 100 × 0 (zero portion)
        fn u32_portion_of_total(
            #[case] value: u32,
            #[case] portion: u64,
            #[case] total: u64,
            #[case] expected: u32,
        ) {
            assert_eq!(value.prorate(portion, total), expected);
        }

        #[test]
        fn returns_zero_for_u32_when_total_is_zero() {
            assert_eq!(100u32.prorate(1, 0), 0u32);
        }

        #[test]
        fn returns_zero_for_f64_when_total_is_zero() {
            assert_eq!(5.0_f64.prorate(1, 0), 0.0_f64);
        }

        #[test]
        fn f32_portion_of_total() {
            let result = 100.0_f32.prorate(1, 4);
            assert!((result - 25.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn f32_returns_zero_when_total_is_zero() {
            assert_eq!(5.0_f32.prorate(1, 0), 0.0_f32);
        }

        #[test]
        fn f64_portion_of_total() {
            let result = 100.0_f64.prorate(1, 4);
            assert!((result - 25.0_f64).abs() < f64::EPSILON);
        }
    }

    // ── Proratable::scale_by ───────────────────────────────────────────────
    mod scale_by {
        use crate::traits::Proratable;
        use rstest::rstest;

        #[rstest]
        #[case(100u32, 0.5_f64, 50u32)]
        #[case(100u32, 1.0_f64, 100u32)]
        #[case(100u32, 0.0_f64, 0u32)]
        #[case(3u32, 0.4_f64, 1u32)] // 1.2 truncates to 1
        fn u32_scale_by_ratio(#[case] value: u32, #[case] ratio: f64, #[case] expected: u32) {
            assert_eq!(value.scale_by(ratio), expected);
        }

        #[test]
        fn f64_scale_by_is_exact_multiplication() {
            assert!((2.0_f64.scale_by(1.5_f64) - 3.0_f64).abs() < f64::EPSILON);
        }

        #[test]
        fn f32_scale_by_is_multiplication() {
            assert!((3.0_f32.scale_by(2.0_f64) - 6.0_f32).abs() < f32::EPSILON);
        }
    }
}
