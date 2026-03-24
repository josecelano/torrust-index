use std::fmt::Debug;

pub trait Accumulator: Copy + PartialOrd + Debug + Default + Send + Sync + 'static {

    fn zero() -> Self;

    #[must_use]
    fn add(self, other: Self) -> Self;

    #[must_use]
    fn sub(self, other: Self) -> Self;
}

macro_rules! impl_accumulator_uint {
    ($($ty:ty),+) => {$(
        impl Accumulator for $ty {
            #[inline]
            fn zero() -> Self { 0 }

            #[inline]
            fn add(self, other: Self) -> Self {
                self.checked_add(other).expect("attempt to add with overflow")
            }

            #[inline]
            fn sub(self, other: Self) -> Self {
                self.checked_sub(other).expect("attempt to subtract with overflow")
            }
        }
    )+};
}

impl_accumulator_uint!(u8, u16, u32, u64, u128);

impl Accumulator for f32 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn add(self, other: Self) -> Self {
        self + other
    }

    #[inline]
    fn sub(self, other: Self) -> Self {
        self - other
    }
}

impl Accumulator for f64 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn add(self, other: Self) -> Self {
        self + other
    }

    #[inline]
    fn sub(self, other: Self) -> Self {
        self - other
    }
}
