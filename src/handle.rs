use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GNodeId(NonZeroU32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VNodeId(NonZeroU32);

macro_rules! impl_handle {
    ($ty:ident) => {
        impl $ty {
            #[doc(hidden)]
            #[must_use]
            pub fn from_index(index: usize) -> Self {
                let raw = u32::try_from(index)
                    .ok()
                    .and_then(|i| i.checked_add(1))
                    .and_then(NonZeroU32::new)
                    .expect(concat!(stringify!($ty), ": index out of range"));
                Self(raw)
            }

            #[doc(hidden)]
            #[must_use]
            #[inline]
            pub const fn index(self) -> usize {
                (self.0.get() - 1) as usize
            }
        }
    };
}

impl_handle!(GNodeId);
impl_handle!(VNodeId);
