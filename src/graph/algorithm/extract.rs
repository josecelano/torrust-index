use crate::graph::{Config, GvGraph};
use crate::handle::VNodeId;
use crate::traits::{Accumulator, Coordinate, Inspectable};

impl<C: Coordinate, V: Accumulator + Inspectable, const N: u32> GvGraph<C, V, N> {
    #[must_use]
    pub fn extract(&self) -> crate::spatial::pewei::Pewei<C, V> {
        use std::collections::VecDeque;

        use crate::nodes::gnode::GState;
        use crate::nodes::vnode::VKind;
        use crate::spatial::pewei::{Layer, Pewei, Terminal, Transition};

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

            match &vnode.kind() {
                VKind::Entry { gnode, .. } => {
                    let g = self.gtree.nodes.get(gnode.index());
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
                                start: g.lo(),
                                end: g.hi(),
                                intensity: g.own(),
                                depth: g_depth,
                                v_depth: bfs_depth,
                            });
                        }
                        GState::SemiInternal | GState::Internal => {
                            layer.transitions.push(Transition {
                                start: g.lo(),
                                end: g.hi(),
                                baseline: g.own(),
                                total: g.sum(),
                                refinement: V::sub(g.sum(), g.own()),
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

    pub fn layers(&self) -> impl Iterator<Item = (usize, crate::spatial::node::Node<C, V>)> + '_ {
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
    type Item = (usize, crate::spatial::node::Node<C, V>);

    fn next(&mut self) -> Option<Self::Item> {
        use crate::nodes::vnode::VKind;

        loop {
            let (vid, bfs_depth) = self.queue.pop_front()?;
            let vnode = self.graph.vnodes.get(vid.index());

            match &vnode.kind() {
                VKind::Structural { children, .. } => {
                    for i in 0..children.len() {
                        let (child_id, _) = children.get(i);
                        self.queue.push_back((child_id, bfs_depth + 1));
                    }
                }
                VKind::Entry { gnode, .. } => {
                    let g = self.graph.gtree.nodes.get(gnode.index());
                    let g_depth = self.graph.gnode_depth(*gnode);
                    let node = crate::spatial::node::Node {
                        start: g.lo(),
                        end: g.hi(),
                        own: g.own(),
                        sum: g.sum(),
                        depth: g_depth,
                        state: g.state(),
                        gnode_id: *gnode,
                        parent: g.parent(),
                    };
                    return Some((bfs_depth, node));
                }
            }
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

    fn fresh_graph() -> G {
        GvGraph::new(make_config())
    }

    // ── extract ──────────────────────────────────────────────────────────
    mod extract {
        use super::*;

        #[test]
        fn fresh_graph_returns_one_terminal_in_layer_zero() {
            let g = fresh_graph();
            let pewei = g.extract();
            assert_eq!(pewei.layer_count(), 1);
            assert_eq!(pewei.layers[0].terminals.len(), 1);
            assert_eq!(pewei.layers[0].transitions.len(), 0);
        }

        #[test]
        fn extract_covers_full_domain() {
            let g = fresh_graph();
            let pewei = g.extract();
            assert_eq!(pewei.domain_start, 0u8);
            use crate::traits::Coordinate;
            assert_eq!(pewei.domain_end, u8::domain_max(8));
        }

        #[test]
        fn single_observation_produces_one_terminal() {
            // delta=2 equals split_threshold so no split is triggered (>2 required)
            let mut g = fresh_graph();
            g.observe(0u8, 2u32);
            let pewei = g.extract();
            assert_eq!(pewei.node_count(), 1);
        }

        #[test]
        fn total_energy_matches_total_sum_after_observe() {
            let mut g = fresh_graph();
            g.observe(0u8, 10u32);
            g.observe(128u8, 20u32);
            g.observe(64u8, 15u32);
            let pewei = g.extract();
            assert_eq!(pewei.total_energy(), g.total_sum());
        }
    }

    // ── layers ───────────────────────────────────────────────────────────
    mod layers {
        use super::*;

        #[test]
        fn fresh_graph_has_exactly_one_node_in_layers() {
            let g = fresh_graph();
            let count = g.layers().count();
            assert_eq!(count, 1);
        }

        #[test]
        fn all_layer_nodes_have_gnode_ids_that_are_valid() {
            let g = fresh_graph();
            for (_depth, node) in g.layers() {
                assert!(g.gtree.nodes.is_occupied(node.gnode_id.index()));
            }
        }
    }

    // ── from_observations ────────────────────────────────────────────────
    mod from_observations {
        use super::*;

        #[test]
        fn produces_same_total_sum_as_sequential_observe() {
            let obs: Vec<(u8, u32)> = vec![(0, 10), (128, 20)];

            let g_batch = G::from_observations(make_config(), obs.clone());

            let mut g_seq = fresh_graph();
            for (coord, delta) in &obs {
                g_seq.observe(*coord, *delta);
            }

            assert_eq!(g_batch.total_sum(), g_seq.total_sum());
        }
    }

    // ── Extend ───────────────────────────────────────────────────────────
    mod extend {
        use super::*;

        #[test]
        fn extend_accumulates_all_observations() {
            let mut g = fresh_graph();
            g.extend([(0u8, 10u32), (128u8, 20u32)]);
            assert_eq!(g.total_sum(), 30u32);
        }
    }

    // ── layers after split covers Structural VNode path ───────────────
    mod layers_after_split {
        use super::*;

        #[test]
        fn layers_on_split_graph_yields_multiple_nodes() {
            let mut g = fresh_graph();
            // Trigger bootstrap split (own=5 > split_threshold=2)
            for _ in 0..3 {
                g.observe(64u8, 5u32);
            }
            let count = g.layers().count();
            assert!(
                count > 1,
                "expected multiple nodes after split, got {count}"
            );
        }

        #[test]
        fn extract_on_split_graph_contains_terminals_and_transitions() {
            let mut g = fresh_graph();
            for _ in 0..3 {
                g.observe(64u8, 5u32);
            }
            let pewei = g.extract();
            // After split there should be at least one layer with terminals
            assert!(pewei.layer_count() >= 1);
            // total_energy should match total_sum
            assert_eq!(pewei.total_energy(), g.total_sum());
        }
    }
}
