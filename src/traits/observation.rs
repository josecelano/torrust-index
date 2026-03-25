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

#[cfg(test)]
mod tests {
    // ── Observation::accumulate ─────────────────────────────────────────────
    mod accumulate {
        use crate::traits::Observation;
        use rstest::rstest;

        // Same-type accumulation: V obs V delegates to V::add.
        #[rstest]
        #[case(0u32, 0u32, 0u32)]
        #[case(10u32, 5u32, 15u32)]
        #[case(u32::MAX - 1, 1u32, u32::MAX)]
        fn same_type_adds_delta(#[case] current: u32, #[case] delta: u32, #[case] expected: u32) {
            assert_eq!(u32::accumulate(current, delta), expected);
        }

        // f64 delta accumulated into a u32 accumulator.
        #[rstest]
        #[case(10u32, 5.0_f64, 15u32)]
        #[case(10u32, 2.9_f64, 12u32)] // 12.9 truncates to 12
        #[case(0u32, 0.0_f64, 0u32)]
        fn f64_delta_added_to_u32(#[case] current: u32, #[case] delta: f64, #[case] expected: u32) {
            assert_eq!(
                <f64 as Observation<u32>>::accumulate(current, delta),
                expected
            );
        }

        #[test]
        fn f64_accumulates_into_f32() {
            let result = <f64 as Observation<f32>>::accumulate(1.0_f32, 2.0_f64);
            assert!((result - 3.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn f32_accumulates_into_u32() {
            assert_eq!(<f32 as Observation<u32>>::accumulate(10u32, 3.0_f32), 13u32);
        }
    }

    // ── ScalableObservation::scale ───────────────────────────────────────────
    mod scale {
        use crate::traits::ScalableObservation;
        use rstest::rstest;

        #[rstest]
        #[case(100u32, 0.5_f64, 50u32)]
        #[case(100u32, 1.0_f64, 100u32)]
        #[case(100u32, 0.0_f64, 0u32)]
        #[case(3u32, 0.4_f64, 1u32)] // 1.2 truncates to 1
        fn f64_scales_u32(#[case] current: u32, #[case] factor: f64, #[case] expected: u32) {
            assert_eq!(
                <f64 as ScalableObservation<u32>>::scale(current, factor),
                expected
            );
        }

        #[test]
        fn f64_scales_f32() {
            let result = <f64 as ScalableObservation<f32>>::scale(4.0_f32, 0.5_f64);
            assert!((result - 2.0_f32).abs() < f32::EPSILON);
        }

        #[test]
        fn f32_scales_u32() {
            assert_eq!(
                <f32 as ScalableObservation<u32>>::scale(100u32, 0.5_f32),
                50u32
            );
        }
    }
}
