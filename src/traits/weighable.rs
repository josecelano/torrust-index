use super::Accumulator;

pub trait Weighable: Accumulator {
    fn weight(self) -> f64;
}

macro_rules! impl_weighable_uint {
    ($($ty:ty),+) => {$(
        impl Weighable for $ty {
            #[inline]
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
            fn weight(self) -> f64 {
                self as f64
            }
        }
    )+};
}

impl_weighable_uint!(u8, u16, u32, u64, u128);

impl Weighable for f32 {
    #[inline]
    fn weight(self) -> f64 {
        f64::from(self)
    }
}

impl Weighable for f64 {
    #[inline]
    fn weight(self) -> f64 {
        self
    }
}

#[cfg(test)]
mod tests {
    // ── Weighable::weight ─────────────────────────────────────────────────────
    mod weight {
        use crate::traits::Weighable;
        use rstest::rstest;

        #[rstest]
        #[case(0u32, 0.0_f64)]
        #[case(100u32, 100.0_f64)]
        #[case(1_000_000u32, 1_000_000.0_f64)]
        fn u32_weight_equals_value_cast_to_f64(#[case] value: u32, #[case] expected: f64) {
            assert!((value.weight() - expected).abs() < f64::EPSILON);
        }

        #[test]
        fn f32_weight_is_losslessly_promoted_to_f64() {
            let v = 1.5_f32;
            assert!((v.weight() - f64::from(v)).abs() < f64::EPSILON);
        }

        #[test]
        fn f64_weight_is_identity() {
            let v = 3.14_f64;
            assert!((v.weight() - v).abs() < f64::EPSILON);
        }
    }
}
