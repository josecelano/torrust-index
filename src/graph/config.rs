use crate::traits::Accumulator;

#[derive(Debug, Clone, PartialEq)]
pub struct Config<V: Accumulator> {
    pub split_threshold: V,

    pub depth_create: u32,

    pub depth_evict: u32,

    pub budget: Option<usize>,

    pub alpha_relax: f64,

    pub bounded_eviction: bool,
}

impl<V: Accumulator> Config<V> {
    pub(crate) fn validate(&self) {
        assert!(
            self.depth_create < self.depth_evict,
            "Config: D_create ({}) must be < D_evict ({}) (idea.md D-I3)",
            self.depth_create,
            self.depth_evict
        );
        assert!(
            self.depth_create >= 1,
            "Config: D_create ({}) must be >= 1",
            self.depth_create
        );
        assert!(
            self.alpha_relax > 0.0 && self.alpha_relax < 1.0,
            "Config: alpha_relax ({}) must be in (0.0, 1.0)",
            self.alpha_relax
        );
        if let Some(budget) = self.budget {
            let buffer = self.depth_evict - self.depth_create;
            let headroom = 3usize.pow(buffer + 1);
            let convergence = 2 * (self.depth_create as usize).saturating_sub(1);
            let required = headroom.max(convergence);
            assert!(
                budget > required,
                "Config: budget ({budget}) must be > max(3^(buffer+1), 2*(D_c-1)) \
                 = {required} (ADR-M-018: budget must exceed required headroom \
                 for hard ceiling guarantee)"
            );
        }
    }
}
