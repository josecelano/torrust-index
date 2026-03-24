use crate::graph::{Config, GvGraph};
use crate::handle::VNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {

    #[must_use]
    pub fn extract(&self) -> crate::pewei::Pewei<C, V> {
        use std::collections::VecDeque;

        use crate::nodes::gnode::GState;
        use crate::pewei::{Layer, Pewei, Terminal, Transition};
        use crate::nodes::vnode::VKind;

        let domain_start = C::zero();
        let domain_end = C::domain_max(N);

        let Some(v_root) = self.v_root else {
            return Pewei {
                domain_start,
                domain_end,
                layers: vec![],
            };
        };

        let mut queue = VecDeque::new();
        queue.push_back((v_root, 0u32));

        let mut layers: Vec<Layer<C, V>> = Vec::new();

        while let Some((vid, bfs_depth)) = queue.pop_front() {
            let vnode = self.vnodes.get(vid.index());

            match &vnode.kind {
                VKind::Entry { gnode, .. } => {
                    let g = self.gnodes.get(gnode.index());
                    let g_depth = self.gnode_depth(*gnode);

                    while layers.len() <= bfs_depth as usize {
                        layers.push(Layer {
                            transitions: Vec::new(),
                            terminals: Vec::new(),
                        });
                    }
                    let layer = &mut layers[bfs_depth as usize];

                    match g.state() {
                        GState::Terminal => {
                            layer.terminals.push(Terminal {
                                start: g.lo,
                                end: g.hi,
                                intensity: g.own,
                                depth: g_depth,
                                v_depth: bfs_depth,
                            });
                        }
                        GState::SemiInternal | GState::Internal => {
                            layer.transitions.push(Transition {
                                start: g.lo,
                                end: g.hi,
                                baseline: g.own,
                                total: g.sum,
                                refinement: V::sub(g.sum, g.own),
                                depth: g_depth,
                                v_depth: bfs_depth,
                            });
                        }
                    }
                }
                VKind::Structural { children, .. } => {
                    for i in 0..children.len() {
                        let (child_id, _intensity) = children.get(i);
                        queue.push_back((child_id, bfs_depth + 1));
                    }
                }
            }
        }

        Pewei {
            domain_start,
            domain_end,
            layers,
        }
    }

    pub fn layers(&self) -> impl Iterator<Item = (usize, crate::view::Node<C, V>)> + '_ {
        let mut queue = std::collections::VecDeque::new();
        if let Some(v_root) = self.v_root {
            queue.push_back((v_root, 0usize));
        }
        Layers { graph: self, queue }
    }

    #[must_use]
    pub fn from_observations<O, I>(config: Config<V>, iter: I) -> Self
    where
        O: crate::traits::Observation<V>,
        I: IntoIterator<Item = (C, O)>,
    {
        let mut graph = Self::new(config);
        graph.extend(iter);
        graph
    }
}

impl<C, V, O, const N: u32> Extend<(C, O)> for GvGraph<C, V, N>
where
    C: Coordinate,
    V: Accumulator + Inspectable,
    O: crate::traits::Observation<V>,
{
    fn extend<I: IntoIterator<Item = (C, O)>>(&mut self, iter: I) {
        for (coord, delta) in iter {
            self.observe(coord, delta);
        }
    }
}

struct Layers<'a, C: Coordinate, V: Accumulator, const N: u32> {
    graph: &'a GvGraph<C, V, N>,
    queue: std::collections::VecDeque<(VNodeId, usize)>,
}

impl<C: Coordinate, V: Accumulator, const N: u32> Iterator for Layers<'_, C, V, N> {
    type Item = (usize, crate::view::Node<C, V>);

    fn next(&mut self) -> Option<Self::Item> {
        use crate::nodes::vnode::VKind;

        loop {
            let (vid, bfs_depth) = self.queue.pop_front()?;
            let vnode = self.graph.vnodes.get(vid.index());

            match &vnode.kind {
                VKind::Structural { children, .. } => {
                    for i in 0..children.len() {
                        let (child_id, _) = children.get(i);
                        self.queue.push_back((child_id, bfs_depth + 1));
                    }

                }
                VKind::Entry { gnode, .. } => {
                    let g = self.graph.gnodes.get(gnode.index());
                    let g_depth = self.graph.gnode_depth(*gnode);
                    let node = crate::view::Node {
                        start: g.lo,
                        end: g.hi,
                        own: g.own,
                        sum: g.sum,
                        depth: g_depth,
                        state: g.state(),
                        gnode_id: *gnode,
                        parent: g.parent,
                    };
                    return Some((bfs_depth, node));
                }
            }
        }
    }
}
