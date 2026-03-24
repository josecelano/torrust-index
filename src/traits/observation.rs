use std::fmt::Debug;

use super::Accumulator;

pub trait Observation<V: Accumulator>: Copy + Debug + Send + Sync {
    fn accumulate(current: V, delta: Self) -> V;
}

pub trait ScalableObservation<V: Accumulator>: Observation<V> {
    fn scale(current: V, factor: Self) -> V;
}

impl<V: Accumulator> Observation<V> for V {
    #[inline]
    fn accumulate(current: V, delta: Self) -> V {
        V::add(current, delta)
    }
}

macro_rules! impl_observation_f64_to_uint {
    ($($v:ty),+) => {$(
        impl Observation<$v> for f64 {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn accumulate(current: $v, delta: Self) -> $v {
                (current as f64 + delta) as $v
            }
        }

        impl ScalableObservation<$v> for f64 {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn scale(current: $v, factor: Self) -> $v {
                (current as f64 * factor) as $v
            }
        }
    )+};
}

impl_observation_f64_to_uint!(u8, u16, u32, u64, u128);

impl Observation<f32> for f64 {
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn accumulate(current: f32, delta: Self) -> f32 {
        (Self::from(current) + delta) as f32
    }
}

impl ScalableObservation<f32> for f64 {
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn scale(current: f32, factor: Self) -> f32 {
        (Self::from(current) * factor) as f32
    }
}

macro_rules! impl_observation_f32_to_uint {
    ($($v:ty),+) => {$(
        impl Observation<$v> for f32 {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn accumulate(current: $v, delta: Self) -> $v {
                (current as f32 + delta) as $v
            }
        }

        impl ScalableObservation<$v> for f32 {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::cast_precision_loss)]
            fn scale(current: $v, factor: Self) -> $v {
                (current as f32 * factor) as $v
            }
        }
    )+};
}

impl_observation_f32_to_uint!(u8, u16, u32);
