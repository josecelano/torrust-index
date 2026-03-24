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
