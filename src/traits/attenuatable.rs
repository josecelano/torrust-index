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
