use crate::spatial::view::Span;
use crate::traits::{Accumulator, Coordinate, Proratable, Weighable};

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
            #[allow(clippy::cast_possible_truncation)]
            output.push(Span {
                start,
                end,
                intensity: accumulated,
                depth: g_depth as u32,
            });
        }
        Some(NodeRef::Terminal { layer, idx }) => {
            let t = &layers[layer as usize].terminals[idx as usize];
            output.push(Span {
                start,
                end,
                intensity: V::add(accumulated, t.intensity),
                depth: t.depth,
            });
        }
        Some(NodeRef::Transition { layer, idx }) => {
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
                    descend(lookup, layers, start, mid, child_depth, half_bg, output);
                    descend(lookup, layers, mid, end, child_depth, half_bg, output);
                }
                (Some(left_nr), None) => {
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Layer<C: Coordinate, V: Accumulator> {
    pub transitions: Vec<Transition<C, V>>,

    pub terminals: Vec<Terminal<C, V>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transition<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub baseline: V,

    pub total: V,

    pub refinement: V,

    pub depth: u32,

    pub v_depth: u32,
}

impl<C: Coordinate, V: Accumulator + Weighable> Transition<C, V> {
    #[must_use]
    pub fn snr(&self) -> Option<f64> {
        let b = self.baseline.weight();
        if b == 0.0 {
            return None;
        }
        Some(self.refinement.weight() / b)
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Terminal<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub intensity: V,

    pub depth: u32,

    pub v_depth: u32,
}

impl<C: Coordinate, V: Accumulator> Terminal<C, V> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> C {
        C::width(self.start, self.end)
    }
}
