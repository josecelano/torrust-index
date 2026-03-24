use std::fmt::Debug;

pub trait Coordinate: Copy + PartialOrd + Debug + Default + Send + Sync + 'static {
    const BITS: u32;

    fn zero() -> Self;

    fn domain_max(n: u32) -> Self;

    fn midpoint(a: Self, b: Self) -> Self;

    fn width(start: Self, end: Self) -> Self;

    fn is_final(start: Self, end: Self, depth: u32, n: u32) -> bool;

    fn from_u64(v: u64) -> Self;

    #[must_use]
    fn next_value(self) -> Self;

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
            fn next_value(self) -> Self {
                self + 1
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
    fn next_value(self) -> Self {
        panic!(
            "next_value is not supported for f32 coordinates; use Excluded/Included bounds directly"
        )
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
    fn next_value(self) -> Self {
        panic!(
            "next_value is not supported for f64 coordinates; use Excluded/Included bounds directly"
        )
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
