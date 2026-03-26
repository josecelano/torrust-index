#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub struct ViolationSources {
    pub source_3_contraction_grandchildren: bool,

    pub source_4_promotion_children: bool,

    pub source_6_leaf_removal_ancestors: bool,

    pub source_7_collapse_children: bool,

    pub source_8_three_to_two_siblings: bool,

    pub source_9_collapse_cousins: bool,

    pub source_10_g_contraction_promotion: bool,
}

impl Default for ViolationSources {
    fn default() -> Self {
        Self::all_enabled()
    }
}

impl ViolationSources {
    #[inline]
    #[must_use]
    pub const fn all_enabled() -> Self {
        Self {
            source_3_contraction_grandchildren: true,
            source_4_promotion_children: true,
            source_6_leaf_removal_ancestors: true,
            source_7_collapse_children: true,
            source_8_three_to_two_siblings: true,
            source_9_collapse_cousins: true,
            source_10_g_contraction_promotion: true,
        }
    }
}
