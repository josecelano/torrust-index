use crate::handle::{GNodeId, VNodeId};

#[derive(Debug, Clone)]
pub struct GNode<C, V> {
    pub(crate) lo: C,

    pub(crate) hi: C,

    pub(crate) sum: V,

    pub(crate) own: V,

    pub(crate) left: Option<GNodeId>,

    pub(crate) right: Option<GNodeId>,

    pub(crate) parent: Option<GNodeId>,

    pub(crate) entry: Option<VNodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GState {
    Terminal,

    SemiInternal,

    Internal,
}

impl<C, V> GNode<C, V> {
    #[inline]
    #[must_use]
    pub const fn state(&self) -> GState {
        match (self.left, self.right) {
            (None, None) => GState::Terminal,
            (Some(_), Some(_)) => GState::Internal,
            _ => GState::SemiInternal,
        }
    }

    #[inline]
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn has_dependents(&self) -> bool {
        self.left.is_some() || self.right.is_some()
    }

    #[inline]
    #[must_use]
    pub const fn is_semi_internal(&self) -> bool {
        matches!(self.state(), GState::SemiInternal)
    }

    #[must_use]
    pub fn uncovered_range(&self) -> Option<(C, C)>
    where
        C: crate::traits::Coordinate,
    {
        match (self.left, self.right) {
            (None, None) => Some((self.lo, self.hi)),
            (Some(_), None) => {
                let mid = C::midpoint(self.lo, self.hi);
                Some((mid, self.hi))
            }
            (None, Some(_)) => {
                let mid = C::midpoint(self.lo, self.hi);
                Some((self.lo, mid))
            }
            (Some(_), Some(_)) => None,
        }
    }
}

impl<C: Default, V: Default> Default for GNode<C, V> {
    fn default() -> Self {
        Self {
            lo: C::default(),
            hi: C::default(),
            sum: V::default(),
            own: V::default(),
            left: None,
            right: None,
            parent: None,
            entry: None,
        }
    }
}
