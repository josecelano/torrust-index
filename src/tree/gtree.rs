use crate::arena::Arena;
use crate::handle::GNodeId;
use crate::nodes::gnode::GNode;
use crate::traits::{Accumulator, Coordinate};

#[must_use]
pub fn route_to_receiver<C: Coordinate, V: Accumulator>(
    gnodes: &Arena<GNode<C, V>>,
    root: GNodeId,
    x: C,
) -> GNodeId {
    let mut current = root;
    loop {
        let g = gnodes.get(current.index());
        let mid = C::midpoint(g.lo(), g.hi());
        if x < mid {
            if let Some(left) = g.left() {
                current = left;
            } else {
                return current;
            }
        } else if let Some(right) = g.right() {
            current = right;
        } else {
            return current;
        }
    }
}

#[must_use]
#[inline]
pub fn gnode_depth_from_interval<C: Coordinate>(lo: C, hi: C, n: u32) -> u32 {
    let width_f64 = C::width(lo, hi).to_f64();
    debug_assert!(
        width_f64 > 0.0,
        "gnode_depth_from_interval: zero-width interval"
    );

    #[allow(clippy::cast_possible_truncation)]
    let log2_width = width_f64.log2() as i32;
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let depth = (n as i32 - log2_width) as u32;
    depth
}

pub fn recompute_g_sums<C: Coordinate, V: Accumulator>(
    gnodes: &mut Arena<GNode<C, V>>,
    start: GNodeId,
) {
    let mut current = Some(start);
    while let Some(id) = current {
        let (left_sum, right_sum) = {
            let g = gnodes.get(id.index());
            let l = g.left().map_or_else(V::zero, |l| gnodes.get(l.index()).sum());
            let r = g.right().map_or_else(V::zero, |r| gnodes.get(r.index()).sum());
            (l, r)
        };
        let g = gnodes.get_mut(id.index());
        g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
        current = g.parent();
    }
}

pub fn recompute_g_sums_subtree<C: Coordinate, V: Accumulator>(
    gnodes: &mut Arena<GNode<C, V>>,
    preorder: &[GNodeId],
) {
    for &gid in preorder.iter().rev() {
        let (left_sum, right_sum) = {
            let g = gnodes.get(gid.index());
            let l = g.left().map_or_else(V::zero, |l| gnodes.get(l.index()).sum());
            let r = g.right().map_or_else(V::zero, |r| gnodes.get(r.index()).sum());
            (l, r)
        };
        let g = gnodes.get_mut(gid.index());
        g.set_sum(V::add(g.own(), V::add(left_sum, right_sum)));
    }
}
