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
