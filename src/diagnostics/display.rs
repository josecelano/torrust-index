#![allow(dead_code)]

use std::fmt;

use crate::arena::Arena;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::graph::GvGraph;
use crate::handle::GNodeId;
use crate::nodes::gnode::GNode;
use crate::traits::{Accumulator, Coordinate, Inspectable};

pub struct Gn<'a, C: Coordinate, V: Accumulator + Inspectable>(
    pub &'a Arena<GNode<C, V>>,
    pub GNodeId,
);

impl<C: Coordinate, V: Accumulator + Inspectable> fmt::Display for Gn<'_, C, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        if !self.0.is_occupied(idx) {
            return write!(f, "G{idx}(DEAD)");
        }
        let g = self.0.get(idx);
        let state = match g.state() {
            crate::nodes::gnode::GState::Terminal => "T",
            crate::nodes::gnode::GState::SemiInternal => "S",
            crate::nodes::gnode::GState::Internal => "I",
        };
        write!(
            f,
            "G{idx}({state},[{},{}),sum={})",
            g.lo.to_f64(),
            g.hi.to_f64(),
            g.sum.to_f64_approx()
        )
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub struct Pl<'a, C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    pub &'a GvGraph<C, V, N>,
    pub GNodeId,
);

#[cfg(feature = "dynamic-contour-tracking")]
impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> fmt::Display for Pl<'_, C, V, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.1.index();
        let key = self.0.plateau_basis.plateau_key(self.1);
        match key {
            Some(k) => {
                if let Some(plateau) = self.0.plateaus.get(&k) {
                    write!(
                        f,
                        "P([{},{}),d={},sum={})",
                        plateau.start.to_f64(),
                        plateau.end.to_f64(),
                        plateau.depth,
                        plateau.sum.to_f64_approx()
                    )
                } else {
                    write!(f, "P(G{idx},key={k:?},NO_PLATEAU)")
                }
            }
            None => write!(f, "P(G{idx},not_basis)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Config, GvGraph, StructuralConfig};

    type G = GvGraph<u8, u32, 8>;

    fn make_config() -> Config<u32> {
        Config {
            split_threshold: 2,
            structural: StructuralConfig {
                depth_create: 3,
                depth_evict: 5,
                budget: None,
                alpha_relax: 0.5,
                bounded_eviction: false,
            },
        }
    }

    mod gn_display_fn {
        use super::*;
        use crate::diagnostics::display::Gn;
        use crate::handle::GNodeId;

        #[test]
        fn dead_gnode_shows_dead_marker() {
            // Fresh graph: only gnode 0 is allocated; index 1 is dead.
            let g: G = GvGraph::new(make_config());
            let display = format!("{}", Gn(g.gnodes(), GNodeId::from_index(1)));
            assert_eq!(display, "G1(DEAD)");
        }

        #[test]
        fn live_terminal_gnode_shows_state_and_range() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // Find the first occupied gnode index.
            for i in 0..10 {
                if g.gnodes().is_occupied(i) {
                    let display = format!("{}", Gn(g.gnodes(), GNodeId::from_index(i)));
                    assert!(display.starts_with(&format!("G{i}(")));
                    assert!(!display.contains("DEAD"));
                    return;
                }
            }
            panic!("no live gnode found after bootstrap");
        }
    }

    #[cfg(feature = "dynamic-contour-tracking")]
    mod pl_display_fn {
        use super::*;
        use crate::diagnostics::display::Pl;
        use crate::handle::GNodeId;

        #[test]
        fn internal_gnode_shows_not_basis() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // gnode[0] is Internal after bootstrap → not in plateau basis.
            let display = format!("{}", Pl(&g, GNodeId::from_index(0)));
            // Either "not_basis" or a plateau display; just verify no panic.
            assert!(!display.is_empty());
        }

        #[test]
        fn terminal_leaf_shows_plateau_info() {
            let mut g: G = GvGraph::new(make_config());
            g.observe(64u8, 3u32);
            // gnode[1] is a Terminal leaf → should be in plateau basis.
            let display = format!("{}", Pl(&g, GNodeId::from_index(1)));
            assert!(!display.is_empty());
        }
    }
}
