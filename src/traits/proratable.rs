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
