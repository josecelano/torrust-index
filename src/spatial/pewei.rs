//! Pewei — layered spatial snapshot of the G-tree.
//!
//! A [`Pewei`] is a read-only, multi-layer representation of the G-tree state
//! captured at successive decay epochs.  Each [`Layer`] records the G-tree
//! regions that were "visible" at a specific epoch:
//!
//! - A [`Terminal`] node is a region that was a leaf in the G-tree at that
//!   epoch.  It holds a single `intensity` value covering the whole
//!   `[start, end)` span.
//! - A [`Transition`] node is an internal region at that epoch.  It holds a
//!   `baseline` (the G-node's own accumulated value) and a `total` (sum of the
//!   entire subtree below it in the most recent layer where it was `Internal`).
//!
//! [`Pewei::reconstruct`] materialises a contiguous list of [`Span`] values
//! covering `[domain_start, domain_end)`.  It uses [`RegionLookup`] to do
//! O(log n) lookups per depth level, and [`descend`] to recursively compose
//! overlapping contributions from multiple layers into smooth spatial spans.
use crate::spatial::view::Span;
use crate::traits::{Accumulator, Coordinate, Proratable};

pub use super::pewei_types::{Layer, Terminal, Transition};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pewei<C: Coordinate, V: Accumulator> {
    pub domain_start: C,

    pub domain_end: C,

    pub layers: Vec<Layer<C, V>>,
}

impl<C: Coordinate, V: Accumulator> Pewei<C, V> {
    #[inline]
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.layers
            .iter()
            .map(|l| l.transitions.len() + l.terminals.len())
            .sum()
    }

    #[must_use]
    pub fn total_energy(&self) -> V {
        let mut acc = V::zero();
        for layer in &self.layers {
            for t in &layer.transitions {
                acc = V::add(acc, t.baseline);
            }
            for t in &layer.terminals {
                acc = V::add(acc, t.intensity);
            }
        }
        acc
    }
}

impl<C: Coordinate, V: Accumulator + Proratable> Pewei<C, V> {
    #[must_use]
    pub fn reconstruct(&self, max_layer: usize) -> Vec<Span<C, V>> {
        if self.layers.is_empty() {
            return vec![Span {
                start: self.domain_start,
                end: self.domain_end,
                intensity: V::zero(),
                depth: 0,
            }];
        }

        let max_layer = max_layer.min(self.layers.len() - 1);
        let visible = &self.layers[..=max_layer];
        let lookup = RegionLookup::build(visible);

        let estimated = visible
            .iter()
            .map(|l| l.terminals.len() + l.transitions.len())
            .sum::<usize>()
            + 1;
        let mut output = Vec::with_capacity(estimated);

        descend(
            &lookup,
            visible,
            self.domain_start,
            self.domain_end,
            0,
            V::zero(),
            &mut output,
        );
        output
    }
}

#[derive(Debug, Clone, Copy)]
enum NodeRef {
    Transition { layer: u32, idx: u32 },
    Terminal { layer: u32, idx: u32 },
}

struct DepthBucket<C: Coordinate> {
    starts: Vec<C>,
    nodes: Vec<NodeRef>,
}

struct RegionLookup<C: Coordinate> {
    by_depth: Vec<DepthBucket<C>>,
}

impl<C: Coordinate> RegionLookup<C> {
    /// Build depth-indexed sorted lookup buckets from `layers`.
    ///
    /// Each `DepthBucket` holds two parallel vecs — `starts` and `nodes` —
    /// sorted by `start` coordinate.  Together they enable an O(log n) binary
    /// search: given a `(start, depth)` pair, `RegionLookup::get` finds the
    /// matching node in the corresponding depth bucket in O(log n) time,
    /// avoiding a linear scan over all nodes at each recursive step in
    /// [`descend`].
    fn build<V: Accumulator>(layers: &[Layer<C, V>]) -> Self {
        let max_depth = layers
            .iter()
            .flat_map(|l| {
                l.transitions
                    .iter()
                    .map(|t| t.depth)
                    .chain(l.terminals.iter().map(|t| t.depth))
            })
            .max()
            .unwrap_or(0) as usize;

        let mut pairs: Vec<Vec<(C, NodeRef)>> = (0..=max_depth).map(|_| Vec::new()).collect();

        #[allow(clippy::cast_possible_truncation)]
        for (li, layer) in layers.iter().enumerate() {
            for (ti, t) in layer.transitions.iter().enumerate() {
                pairs[t.depth as usize].push((
                    t.start,
                    NodeRef::Transition {
                        layer: li as u32,
                        idx: ti as u32,
                    },
                ));
            }
            for (ti, t) in layer.terminals.iter().enumerate() {
                pairs[t.depth as usize].push((
                    t.start,
                    NodeRef::Terminal {
                        layer: li as u32,
                        idx: ti as u32,
                    },
                ));
            }
        }

        let by_depth = pairs
            .into_iter()
            .map(|mut bucket| {
                bucket.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let (starts, nodes) = bucket.into_iter().unzip();
                DepthBucket { starts, nodes }
            })
            .collect();

        Self { by_depth }
    }

    fn get(&self, start: C, g_depth: usize) -> Option<NodeRef> {
        let bucket = self.by_depth.get(g_depth)?;

        let idx = bucket
            .starts
            .binary_search_by(|s| s.partial_cmp(&start).unwrap())
            .ok()?;
        Some(bucket.nodes[idx])
    }
}

fn node_total<C: Coordinate, V: Accumulator>(layers: &[Layer<C, V>], nr: NodeRef) -> V {
    match nr {
        NodeRef::Terminal { layer, idx } => {
            layers[layer as usize].terminals[idx as usize].intensity
        }
        NodeRef::Transition { layer, idx } => {
            layers[layer as usize].transitions[idx as usize].total
        }
    }
}

fn descend<C: Coordinate, V: Accumulator + Proratable>(
    lookup: &RegionLookup<C>,
    layers: &[Layer<C, V>],
    start: C,
    end: C,
    g_depth: usize,
    accumulated: V,
    output: &mut Vec<Span<C, V>>,
) {
    match lookup.get(start, g_depth) {
        None => {
            // No node covers this region at this depth across any layer; emit
            // a uniform span carrying only the background accumulated so far.
            #[allow(clippy::cast_possible_truncation)]
            output.push(Span {
                start,
                end,
                intensity: accumulated,
                depth: g_depth as u32,
            });
        }
        Some(NodeRef::Terminal { layer, idx }) => {
            // A leaf node — no children exist.  Add its intensity to the
            // accumulated background and emit a single span for [start, end).
            let t = &layers[layer as usize].terminals[idx as usize];
            output.push(Span {
                start,
                end,
                intensity: V::add(accumulated, t.intensity),
                depth: t.depth,
            });
        }
        Some(NodeRef::Transition { layer, idx }) => {
            // An internal node — may have 0, 1, or 2 children in the lookup.
            // `baseline` is the node's own (non-child) energy contribution;
            // `total` is the full subtree energy (baseline + all descendants).
            let tr = &layers[layer as usize].transitions[idx as usize];
            let baseline = tr.baseline;
            let total = tr.total;
            let depth = tr.depth;

            let mid = C::midpoint(start, end);
            let total_bg = V::add(accumulated, baseline);
            let half_bg = total_bg.prorate(1, 2);
            let child_depth = g_depth + 1;

            let left_ref = lookup.get(start, child_depth);
            let right_ref = lookup.get(mid, child_depth);

            match (left_ref, right_ref) {
                (Some(_), Some(_)) => {
                    // Both children are present — recurse into each half.
                    descend(lookup, layers, start, mid, child_depth, half_bg, output);
                    descend(lookup, layers, mid, end, child_depth, half_bg, output);
                }
                (Some(left_nr), None) => {
                    // Only the left child is present.  Recurse left; for the
                    // right half emit a uniform span carrying the energy that
                    // cannot be attributed to either the baseline or the known
                    // left child: remainder = total − baseline − left_total.
                    descend(lookup, layers, start, mid, child_depth, half_bg, output);
                    let left_total = node_total(layers, left_nr);
                    let remainder = V::sub(V::sub(total, baseline), left_total);
                    output.push(Span {
                        start: mid,
                        end,
                        intensity: V::add(half_bg, remainder),
                        depth,
                    });
                }
                (None, Some(right_nr)) => {
                    // Only the right child is present.  Emit a uniform span for
                    // the left half (remainder = total − baseline − right_total)
                    // then recurse right.
                    let right_total = node_total(layers, right_nr);
                    let remainder = V::sub(V::sub(total, baseline), right_total);
                    output.push(Span {
                        start,
                        end: mid,
                        intensity: V::add(half_bg, remainder),
                        depth,
                    });
                    descend(lookup, layers, mid, end, child_depth, half_bg, output);
                }
                (None, None) => {
                    // Neither child is present — emit the whole region as a
                    // single span with the full subtree energy.
                    output.push(Span {
                        start,
                        end,
                        intensity: V::add(accumulated, total),
                        depth,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Layer, Pewei, Terminal, Transition};
    use crate::spatial::view::Span;

    fn make_pewei(layers: Vec<Layer<u8, u32>>) -> Pewei<u8, u32> {
        Pewei {
            domain_start: 0,
            domain_end: 16,
            layers,
        }
    }

    fn make_terminal(start: u8, end: u8, intensity: u32) -> Terminal<u8, u32> {
        Terminal {
            start,
            end,
            intensity,
            depth: 0,
            v_depth: 0,
        }
    }

    fn make_transition(start: u8, end: u8, baseline: u32, total: u32) -> Transition<u8, u32> {
        Transition {
            start,
            end,
            baseline,
            total,
            refinement: 0,
            depth: 0,
            v_depth: 0,
        }
    }

    // ── Pewei::layer_count ───────────────────────────────────────────────
    mod layer_count {
        use super::*;

        #[test]
        fn returns_zero_for_empty_layers() {
            assert_eq!(make_pewei(vec![]).layer_count(), 0);
        }

        #[test]
        fn returns_count_of_layers() {
            let empty = Layer {
                transitions: vec![],
                terminals: vec![],
            };
            let p = make_pewei(vec![empty.clone(), empty]);
            assert_eq!(p.layer_count(), 2);
        }
    }

    // ── Pewei::node_count ────────────────────────────────────────────────
    mod node_count {
        use super::*;

        #[test]
        fn returns_zero_for_empty_pewei() {
            assert_eq!(make_pewei(vec![]).node_count(), 0);
        }

        #[test]
        fn counts_transitions_and_terminals_across_layers() {
            let layer0 = Layer {
                transitions: vec![make_transition(0, 16, 5, 15)],
                terminals: vec![],
            };
            let layer1 = Layer {
                transitions: vec![],
                terminals: vec![make_terminal(0, 8, 10), make_terminal(8, 16, 10)],
            };
            let p = make_pewei(vec![layer0, layer1]);
            assert_eq!(p.node_count(), 3);
        }
    }

    // ── Pewei::total_energy ──────────────────────────────────────────────
    mod total_energy {
        use super::*;

        #[test]
        fn returns_zero_for_empty_pewei() {
            assert_eq!(make_pewei(vec![]).total_energy(), 0u32);
        }

        #[test]
        fn sums_all_baselines_and_intensities() {
            let layer = Layer {
                transitions: vec![make_transition(0, 16, 10, 30)],
                terminals: vec![make_terminal(0, 8, 5), make_terminal(8, 16, 7)],
            };
            let p = make_pewei(vec![layer]);
            // baseline(10) + intensity(5) + intensity(7) = 22
            assert_eq!(p.total_energy(), 22u32);
        }
    }

    // ── Pewei::reconstruct ───────────────────────────────────────────────
    mod reconstruct {
        use super::*;

        #[test]
        fn returns_single_zero_span_for_empty_layers() {
            let p = make_pewei(vec![]);
            let result = p.reconstruct(0);
            assert_eq!(
                result,
                vec![Span {
                    start: 0u8,
                    end: 16u8,
                    intensity: 0u32,
                    depth: 0
                }]
            );
        }

        // descend: Terminal branch — node at root depth is a terminal
        #[test]
        fn terminal_at_root_depth() {
            let layer = Layer {
                transitions: vec![],
                terminals: vec![Terminal {
                    start: 0,
                    end: 16,
                    intensity: 42,
                    depth: 0,
                    v_depth: 0,
                }],
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![layer],
            };
            let result = p.reconstruct(0);
            assert_eq!(
                result,
                vec![Span {
                    start: 0u8,
                    end: 16u8,
                    intensity: 42u32,
                    depth: 0
                }]
            );
        }

        // descend: Transition branch, (None, None) — no children in lookup
        #[test]
        fn transition_with_no_children() {
            let root = Transition {
                start: 0u8,
                end: 16u8,
                baseline: 10u32,
                total: 30u32,
                refinement: 20u32,
                depth: 0,
                v_depth: 0,
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![Layer {
                    transitions: vec![root],
                    terminals: vec![],
                }],
            };
            // max_layer=0: lookup has only depth-0 nodes; child lookup returns None,None
            let result = p.reconstruct(0);
            assert_eq!(
                result,
                vec![Span {
                    start: 0u8,
                    end: 16u8,
                    intensity: 30u32,
                    depth: 0
                }]
            );
        }

        // descend: Transition branch, (Some, Some) — both terminal children present
        #[test]
        fn transition_with_both_terminal_children() {
            let root = Transition {
                start: 0u8,
                end: 16u8,
                baseline: 10u32,
                total: 30u32,
                refinement: 20u32,
                depth: 0,
                v_depth: 0,
            };
            let left_t = Terminal {
                start: 0u8,
                end: 8u8,
                intensity: 10u32,
                depth: 1,
                v_depth: 0,
            };
            let right_t = Terminal {
                start: 8u8,
                end: 16u8,
                intensity: 10u32,
                depth: 1,
                v_depth: 0,
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![
                    Layer {
                        transitions: vec![root],
                        terminals: vec![],
                    },
                    Layer {
                        transitions: vec![],
                        terminals: vec![left_t, right_t],
                    },
                ],
            };
            let result = p.reconstruct(1);
            // half_bg = 10.prorate(1,2) = 5; each child: 5+10=15
            assert_eq!(result.len(), 2);
            assert_eq!(
                result[0],
                Span {
                    start: 0u8,
                    end: 8u8,
                    intensity: 15u32,
                    depth: 1
                }
            );
            assert_eq!(
                result[1],
                Span {
                    start: 8u8,
                    end: 16u8,
                    intensity: 15u32,
                    depth: 1
                }
            );
        }

        // descend: Transition branch, (Some, None) — left terminal child only
        // also covers node_total Terminal arm
        #[test]
        fn transition_with_left_terminal_child_only() {
            let root = Transition {
                start: 0u8,
                end: 16u8,
                baseline: 10u32,
                total: 30u32,
                refinement: 0u32,
                depth: 0,
                v_depth: 0,
            };
            let left_t = Terminal {
                start: 0u8,
                end: 8u8,
                intensity: 12u32,
                depth: 1,
                v_depth: 0,
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![
                    Layer {
                        transitions: vec![root],
                        terminals: vec![],
                    },
                    Layer {
                        transitions: vec![],
                        terminals: vec![left_t],
                    },
                ],
            };
            let result = p.reconstruct(1);
            // half_bg=5; left=5+12=17; remainder=30-10-12=8; right=5+8=13
            assert_eq!(result.len(), 2);
            assert_eq!(
                result[0],
                Span {
                    start: 0u8,
                    end: 8u8,
                    intensity: 17u32,
                    depth: 1
                }
            );
            assert_eq!(
                result[1],
                Span {
                    start: 8u8,
                    end: 16u8,
                    intensity: 13u32,
                    depth: 0
                }
            );
        }

        // descend: Transition branch, (None, Some) — right terminal child only
        #[test]
        fn transition_with_right_terminal_child_only() {
            let root = Transition {
                start: 0u8,
                end: 16u8,
                baseline: 10u32,
                total: 30u32,
                refinement: 0u32,
                depth: 0,
                v_depth: 0,
            };
            let right_t = Terminal {
                start: 8u8,
                end: 16u8,
                intensity: 12u32,
                depth: 1,
                v_depth: 0,
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![
                    Layer {
                        transitions: vec![root],
                        terminals: vec![],
                    },
                    Layer {
                        transitions: vec![],
                        terminals: vec![right_t],
                    },
                ],
            };
            let result = p.reconstruct(1);
            // half_bg=5; right_total=12; remainder=30-10-12=8; left=5+8=13; right=5+12=17
            assert_eq!(result.len(), 2);
            assert_eq!(
                result[0],
                Span {
                    start: 0u8,
                    end: 8u8,
                    intensity: 13u32,
                    depth: 0
                }
            );
            assert_eq!(
                result[1],
                Span {
                    start: 8u8,
                    end: 16u8,
                    intensity: 17u32,
                    depth: 1
                }
            );
        }

        // descend: (Some, None) where the child present is itself a Transition
        // covers node_total Transition arm
        #[test]
        fn transition_with_left_transition_child_only() {
            let root = Transition {
                start: 0u8,
                end: 16u8,
                baseline: 10u32,
                total: 30u32,
                refinement: 0u32,
                depth: 0,
                v_depth: 0,
            };
            // left child is itself a Transition (no grandchildren in lookup)
            let left_tr = Transition {
                start: 0u8,
                end: 8u8,
                baseline: 5u32,
                total: 15u32,
                refinement: 0u32,
                depth: 1,
                v_depth: 0,
            };
            let p = Pewei {
                domain_start: 0u8,
                domain_end: 16u8,
                layers: vec![
                    Layer {
                        transitions: vec![root],
                        terminals: vec![],
                    },
                    Layer {
                        transitions: vec![left_tr],
                        terminals: vec![],
                    },
                ],
            };
            let result = p.reconstruct(1);
            // root descend(0,16,0,0):
            //   transition depth=0, mid=8, total_bg=10, half_bg=5, child_depth=1
            //   left_ref=Some(Transition{l:1,i:0}), right_ref=None → (Some, None)
            //   descend(0,8,1,5): left_tr depth=1, mid=4, total_bg=10, half_bg=5, child_depth=2
            //     both None → push Span{0,8, 5+15=20, depth=1}
            //   left_total = node_total(Transition{l:1,i:0}) = layers[1].transitions[0].total = 15
            //   remainder = 30-10-15 = 5
            //   push Span{8,16, 5+5=10, depth=0}
            assert_eq!(result.len(), 2);
            assert_eq!(
                result[0],
                Span {
                    start: 0u8,
                    end: 8u8,
                    intensity: 20u32,
                    depth: 1
                }
            );
            assert_eq!(
                result[1],
                Span {
                    start: 8u8,
                    end: 16u8,
                    intensity: 10u32,
                    depth: 0
                }
            );
        }
    }

    // ── Transition::snr ──────────────────────────────────────────────────
    mod transition_snr {
        use super::*;

        #[test]
        fn none_when_baseline_is_zero() {
            let t = Transition::<u8, u32> {
                start: 0,
                end: 16,
                baseline: 0,
                total: 10,
                refinement: 5,
                depth: 0,
                v_depth: 0,
            };
            assert_eq!(t.snr(), None);
        }

        #[test]
        fn some_when_baseline_nonzero() {
            let t = Transition::<u8, u32> {
                start: 0,
                end: 16,
                baseline: 5,
                total: 15,
                refinement: 10,
                depth: 0,
                v_depth: 0,
            };
            // refinement.weight() / baseline.weight() = 10.0 / 5.0 = 2.0
            assert!((t.snr().unwrap() - 2.0_f64).abs() < 1e-9);
        }
    }

    // ── Terminal::width ──────────────────────────────────────────────────
    mod terminal_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            let t = make_terminal(4, 12, 0);
            assert_eq!(t.width(), 8u8);
        }
    }

    // ── Transition::width ────────────────────────────────────────────────
    mod transition_width {
        use super::*;

        #[test]
        fn returns_end_minus_start() {
            let t = Transition::<u8, u32> {
                start: 0,
                end: 16,
                baseline: 0,
                total: 0,
                refinement: 0,
                depth: 0,
                v_depth: 0,
            };
            assert_eq!(t.width(), 16u8);
        }
    }
}
