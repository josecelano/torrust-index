use std::collections::BTreeMap;

use crate::handle::GNodeId;
use crate::spatial::plateau::{BasisEdge, Plateau};
use crate::traits::{Accumulator, Coordinate};
#[cfg(debug_assertions)]
use crate::traits::{Inspectable, Proratable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BasisElement<C: Coordinate, V: Accumulator> {
    pub gnode_id: GNodeId,

    pub start: C,

    pub end: C,

    pub own: V,

    pub sum: V,

    pub depth: u32,

    pub is_boundary_thatch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContourRange<C: Coordinate, V: Accumulator> {
    pub start: C,

    pub end: C,

    pub basis: Vec<BasisElement<C, V>>,

    pub energy: V,

    pub exact_energy: V,

    pub plateau_energy: V,

    pub cross_plateau_energy: V,

    pub plateau_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContourRangeEnergy<V: Accumulator> {
    pub energy: V,

    pub exact_energy: V,

    pub plateau_energy: V,

    pub cross_plateau_energy: V,

    pub plateau_count: usize,
}

pub fn validate_endpoints<C: Coordinate, V: Accumulator>(
    plateaus: &BTreeMap<BasisEdge<C>, Plateau<C, V>>,
    start: BasisEdge<C>,
    end: BasisEdge<C>,
    domain_end: C,
) -> Option<()> {
    if !plateaus.contains_key(&start) {
        return None;
    }

    if end != BasisEdge(domain_end) && !plateaus.contains_key(&end) {
        return None;
    }

    if start >= end {
        return None;
    }
    Some(())
}

pub fn compute_plateau_energy<C: Coordinate, V: Accumulator>(
    plateaus: &BTreeMap<BasisEdge<C>, Plateau<C, V>>,
    start: BasisEdge<C>,
    end: BasisEdge<C>,
) -> (V, usize) {
    let mut sum = V::zero();
    let mut count = 0usize;
    for (_, p) in plateaus.range(start..end) {
        sum = V::add(sum, p.sum);
        count += 1;
    }
    (sum, count)
}

#[cfg(debug_assertions)]
#[allow(clippy::float_cmp)]
pub fn debug_assert_contour_range_invariants<C, V>(cr: &ContourRange<C, V>)
where
    C: Coordinate,
    V: Accumulator + Proratable + Inspectable,
{
    let thatch_count = cr.basis.iter().filter(|b| b.is_boundary_thatch).count();
    debug_assert!(
        thatch_count <= 2,
        "CR-I8: boundary thatch count = {} > 2 for [{:?}, {:?})",
        thatch_count,
        cr.start,
        cr.end,
    );

    let mut sorted: Vec<_> = cr.basis.iter().collect();
    sorted.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for pair in sorted.windows(2) {
        debug_assert!(
            pair[0].end <= pair[1].start,
            "CR-I3: overlapping basis elements [{:?}, {:?}) and [{:?}, {:?})",
            pair[0].start,
            pair[0].end,
            pair[1].start,
            pair[1].end,
        );
    }

    for pair in sorted.windows(2) {
        debug_assert!(
            pair[0].end >= pair[1].start,
            "CR-I2: gap between basis tiles [{:?}, {:?}) and [{:?}, {:?})",
            pair[0].start,
            pair[0].end,
            pair[1].start,
            pair[1].end,
        );
    }

    let sum = cr.basis.iter().fold(V::zero(), |acc, b| V::add(acc, b.sum));
    let energy_f64 = cr.energy.to_f64_approx();
    let sum_f64 = sum.to_f64_approx();
    debug_assert!(
        energy_f64 == sum_f64 || (energy_f64 - sum_f64).abs() < 1e-10,
        "§CR.6: energy ({energy_f64}) != Σ basis.sum ({sum_f64})",
    );

    let cross_f64 = cr.cross_plateau_energy.to_f64_approx();
    let plateau_f64 = cr.plateau_energy.to_f64_approx();
    let expected_cross = energy_f64 - plateau_f64;

    if !expected_cross.is_nan() {
        debug_assert!(
            cross_f64 == expected_cross || (cross_f64 - expected_cross).abs() < 1e-10,
            "cross_plateau_energy ({cross_f64}) != energy - plateau_energy ({expected_cross})",
        );
    }
}
