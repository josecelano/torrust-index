use crate::graph::GvGraph;
use crate::handle::VNodeId;
use crate::traits::{Accumulator, Coordinate, Weighable};

impl<C: Coordinate, V: Accumulator + Weighable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    #[allow(clippy::doc_markdown)]
    pub fn sample(
        &self,
        rng: &mut impl crate::traits::Rng,
    ) -> Option<crate::spatial::view::Cell<C, V>> {
        use crate::nodes::vnode::VKind;

        let v_root = self.v_root?;
        let root_node = self.vnodes.get(v_root.index());
        if root_node.intensity == V::zero() {
            return None;
        }

        let mut current = v_root;
        loop {
            let vnode = self.vnodes.get(current.index());
            match &vnode.kind {
                VKind::Entry { gnode, .. } => {
                    let g = self.gnodes.get(gnode.index());
                    let (start, end) = Self::uncovered_interval(g);
                    return Some(crate::spatial::view::Cell {
                        start,
                        end,
                        intensity: g.own,
                        depth: crate::tree::gtree::gnode_depth_from_interval(start, end, N),
                    });
                }
                VKind::Structural { children, .. } => {
                    current = Self::sample_child(children, rng);
                }
            }
        }
    }

    fn sample_child(
        children: &crate::nodes::vnode::PackedChildren<V>,
        rng: &mut impl crate::traits::Rng,
    ) -> VNodeId {
        let total: f64 = children.intensities[..children.len()]
            .iter()
            .map(|v| v.weight())
            .sum();
        debug_assert!(total > 0.0, "sample_child: zero-total children");

        let threshold = rng.next_f64() * total;
        let mut cumulative = 0.0_f64;
        for i in 0..children.len() {
            cumulative += children.intensities[i].weight();
            if threshold < cumulative {
                return children.ids[i].expect("child ID within len");
            }
        }

        children.ids[children.len() - 1].expect("last child ID")
    }
}
