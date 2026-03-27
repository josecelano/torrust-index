#[cfg(feature = "dynamic-contour-tracking")]
use crate::graph::{GvGraph, uniform_contour_depth_of};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::nodes::gnode::{GNode, GState};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::spatial::plateau::BasisEdge;
#[cfg(feature = "dynamic-contour-tracking")]
use crate::traits::{Accumulator, Coordinate, Inspectable};
#[cfg(feature = "dynamic-contour-tracking")]
use crate::tree::gtree::gnode_depth_from_interval;

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_plateau_btreemap_key_consistency<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    for (&key, plateau) in &graph.plateaus {
        if plateau.basis_edge != key {
            errors.push(format!(
                "Plateau key consistency: BTreeMap key {key:?} != plateau.basis_edge {:?}",
                plateau.basis_edge
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_plateau_basis_consistency<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();

    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                errors.push(format!(
                    "Plateau basis: basis element {gid:?} in plateau {key:?} is not a live arena slot"
                ));
                continue;
            }

            match pb.plateau_key(gid) {
                Some(back_key) if back_key == key => {}
                Some(back_key) => {
                    errors.push(format!(
                        "Plateau basis forward→back: {gid:?} in forward[{key:?}] but back[{gid:?}] = {back_key:?}"
                    ));
                }
                None => {
                    errors.push(format!(
                        "Plateau basis forward→back: {gid:?} in forward[{key:?}] but not in back map"
                    ));
                }
            }
        }
    }

    for (&gid, &key) in pb.back_map() {
        let elements = pb.basis_elements(&key);
        if !elements.contains(&gid) {
            errors.push(format!(
                "Plateau basis back→forward: back[{gid:?}] = {key:?} but {gid:?} not in forward[{key:?}]"
            ));
        }
    }

    let btree_keys: Vec<_> = graph.plateaus.keys().copied().collect();
    let basis_keys: Vec<_> = pb.iter().map(|(&k, _)| k).collect();
    if btree_keys != basis_keys {
        errors.push(format!(
            "Plateau basis: plateaus.keys() ({} entries) != plateau_basis.forward.keys() ({} entries)",
            btree_keys.len(),
            basis_keys.len()
        ));
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
#[allow(clippy::float_cmp)]
pub fn check_plateau_sum_consistency<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, plateau) in &graph.plateaus {
        let expected: f64 = pb
            .basis_elements(&key)
            .iter()
            .filter(|&&gid| graph.gnodes().is_occupied(gid.index()))
            .map(|&gid| graph.gnodes().get(gid.index()).sum().to_f64_approx())
            .sum();
        let actual = plateau.sum.to_f64_approx();
        if expected != actual && (expected - actual).abs() > 1e-9 {
            errors.push(format!(
                "Plateau sum: key {key:?}: expected sum={expected}, actual sum={actual}"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_plateau_depth_consistency<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, plateau) in &graph.plateaus {
        for &gid in pb.basis_elements(&key) {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let g_depth = gnode_depth_from_interval(g.lo(), g.hi(), N);
            let expected_depth = match g.state() {
                GState::Terminal | GState::SemiInternal => g_depth,
                GState::Internal => {
                    let Some(d) = uniform_contour_depth_of(graph.gnodes(), gid, N) else {
                        errors.push(format!(
                            "Plateau depth: key {key:?}, basis element {gid:?} (Internal): \
                             uniform_contour_depth_of returned None — internal basis \
                             element has non-uniform contour depth",
                        ));
                        continue;
                    };
                    d
                }
            };
            if expected_depth != plateau.depth {
                errors.push(format!(
                    "Plateau depth: key {key:?}, basis element {gid:?} ({:?}): \
                     element contributes depth {expected_depth}, plateau.depth={}",
                    g.state(),
                    plateau.depth,
                ));
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn tile_of<C: Coordinate, V: Accumulator>(g: &GNode<C, V>) -> (C, C) {
    match g.state() {
        GState::Terminal | GState::Internal => (g.lo(), g.hi()),
        GState::SemiInternal => g
            .uncovered_range()
            .expect("semi-internal must have uncovered range"),
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn contour_steps<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
) -> Vec<(C, u32)> {
    let mut cells: Vec<(C, u32)> = Vec::new();
    let mut stack = vec![graph.g_root()];
    while let Some(gid) = stack.pop() {
        let g = graph.gnodes().get(gid.index());
        match g.state() {
            GState::Terminal => {
                let d = gnode_depth_from_interval(g.lo(), g.hi(), N);
                cells.push((g.lo(), d));
            }
            GState::SemiInternal => {
                let (ulo, _uhi) = g.uncovered_range().unwrap();
                let d = gnode_depth_from_interval(g.lo(), g.hi(), N);
                cells.push((ulo, d));

                if let Some(l) = g.left() {
                    stack.push(l);
                }
                if let Some(r) = g.right() {
                    stack.push(r);
                }
            }
            GState::Internal => {
                if let Some(l) = g.left() {
                    stack.push(l);
                }
                if let Some(r) = g.right() {
                    stack.push(r);
                }
            }
        }
    }
    cells.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut steps: Vec<(C, u32)> = Vec::new();
    for &(lo, depth) in &cells {
        if steps.is_empty() || steps.last().unwrap().1 != depth {
            steps.push((lo, depth));
        }
    }
    steps
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i1_i_keys_are_contour_steps<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    if plateaus.is_empty() {
        errors.push("P-I1(i): plateaus BTreeMap is empty (must have at least 1 plateau)".into());
        return;
    }

    let steps = contour_steps(graph);
    let btree_keys: Vec<C> = plateaus.keys().map(|k| k.0).collect();
    let step_coords: Vec<C> = steps.iter().map(|&(c, _)| c).collect();

    if btree_keys.len() != step_coords.len() {
        errors.push(format!(
            "P-I1(i): plateau count ({}) != contour step count ({})",
            btree_keys.len(),
            step_coords.len()
        ));
    }

    for (i, (&(coord, depth), btree_coord)) in steps.iter().zip(btree_keys.iter()).enumerate() {
        if coord.total_cmp(btree_coord) != std::cmp::Ordering::Equal {
            errors.push(format!(
                "P-I1(i): step {i}: contour step at {coord:?}, BTreeMap key at {btree_coord:?}"
            ));
        }

        if let Some(plateau) = plateaus.get(&BasisEdge(coord)) {
            if plateau.depth != depth {
                errors.push(format!(
                    "P-I1(i): step {i} at {coord:?}: contour depth={depth}, plateau.depth={}",
                    plateau.depth
                ));
            }
        }
    }

    let depths: Vec<u32> = plateaus.values().map(|p| p.depth).collect();
    for w in depths.windows(2) {
        if w[0] == w[1] {
            let keys: Vec<_> = plateaus.keys().collect();
            let idx = depths.windows(2).position(|d| d[0] == d[1]).unwrap();
            errors.push(format!(
                "P-I1(i) maximality: consecutive plateaus {:?} and {:?} both have depth {}",
                keys[idx],
                keys[idx + 1],
                w[0]
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i1_ii_tile_contiguity<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    let pb = graph.plateau_basis();
    let keys: Vec<BasisEdge<C>> = plateaus.keys().copied().collect();
    let domain_max = C::domain_max(N);

    for (i, &key) in keys.iter().enumerate() {
        let next_start = if i + 1 < keys.len() {
            keys[i + 1].0
        } else {
            domain_max
        };

        let elements = pb.basis_elements(&key);
        if elements.is_empty() {
            errors.push(format!("P-I1(ii): plateau {key:?} has no basis elements"));
            continue;
        }

        let mut tiles: Vec<(f64, f64)> = Vec::new();
        for &gid in elements {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let (tlo, thi) = tile_of(g);
            tiles.push((tlo.to_f64(), thi.to_f64()));
        }
        tiles.sort_by(|a, b| a.0.total_cmp(&b.0));

        let expected_lo = key.0.to_f64();
        let expected_hi = next_start.to_f64();

        if tiles.is_empty() {
            errors.push(format!("P-I1(ii): plateau {key:?}: no live basis tiles"));
            continue;
        }

        if (tiles[0].0 - expected_lo).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(ii): plateau {key:?}: tile union starts at {}, expected {expected_lo}",
                tiles[0].0
            ));
        }

        let last_hi = tiles.last().unwrap().1;
        if (last_hi - expected_hi).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(ii): plateau {key:?}: tile union ends at {last_hi}, expected {expected_hi}"
            ));
        }

        for w in tiles.windows(2) {
            if w[1].0 - w[0].1 > 1e-12 {
                errors.push(format!(
                    "P-I1(ii): plateau {key:?}: gap in tiles between {} and {}",
                    w[0].1, w[1].0
                ));
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i1_iii_run_contains_tile<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let plateaus = &graph.plateaus;
    let pb = graph.plateau_basis();
    let keys: Vec<BasisEdge<C>> = plateaus.keys().copied().collect();
    let domain_max = C::domain_max(N);

    for (i, &key) in keys.iter().enumerate() {
        let plateau = &plateaus[&key];
        let next_start = if i + 1 < keys.len() {
            keys[i + 1].0
        } else {
            domain_max
        };

        let elements = pb.basis_elements(&key);
        if elements.is_empty() {
            continue;
        }

        let mut min_lo = f64::INFINITY;
        let mut max_hi = f64::NEG_INFINITY;
        for &gid in elements {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let lo = g.lo().to_f64();
            let hi = g.hi().to_f64();
            if lo < min_lo {
                min_lo = lo;
            }
            if hi > max_hi {
                max_hi = hi;
            }
        }

        let p_start = plateau.start.to_f64();
        let p_end = plateau.end.to_f64();
        if (p_start - min_lo).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: start={p_start}, expected min(basis.lo())={min_lo}"
            ));
        }
        if (p_end - max_hi).abs() > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: end={p_end}, expected max(basis.hi())={max_hi}"
            ));
        }

        let tile_lo = key.0.to_f64();
        let tile_hi = next_start.to_f64();
        if p_start - tile_lo > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: run start {p_start} > tile start {tile_lo}"
            ));
        }
        if tile_hi - p_end > 1e-12 {
            errors.push(format!(
                "P-I1(iii): plateau {key:?}: run end {p_end} < tile end {tile_hi}"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i2_basis_minimality<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, gnodes_list) in pb.iter() {
        let Some(plateau) = graph.plateaus.get(&key) else {
            continue;
        };
        let expected_depth = plateau.depth;

        for &gid in gnodes_list {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());

            if g.state() == GState::SemiInternal {
                continue;
            }

            if let Some(parent_id) = g.parent() {
                if let Some(parent_depth) = uniform_contour_depth_of(graph.gnodes(), parent_id, N) {
                    if parent_depth == expected_depth {
                        errors.push(format!(
                            "P-I2 minimality: basis element G({}) in plateau {key:?} \
                             has parent G({}) with uniform contour depth {parent_depth} \
                             == plateau depth {expected_depth} — parent should be the \
                             basis element instead",
                            gid.index(),
                            parent_id.index(),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i3_basis_disjointness<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    let mut tiles: Vec<(f64, f64, BasisEdge<C>)> = Vec::new();
    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            let (tlo, thi) = tile_of(g);
            tiles.push((tlo.to_f64(), thi.to_f64(), key));
        }
    }
    tiles.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));

    for w in tiles.windows(2) {
        let (lo1, hi1, k1) = &w[0];
        let (lo2, _hi2, k2) = &w[1];
        if k1 != k2 && *hi1 > *lo2 + 1e-12 {
            errors.push(format!(
                "P-I3 tile disjointness: tile in plateau {k1:?} [{lo1}, {hi1}) \
                 overlaps tile in plateau {k2:?} [{lo2}, ..)"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i4_thatch_one_hop<
    C: Coordinate,
    V: Accumulator + Inspectable,
    const N: u32,
>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();
    for (&key, gnodes) in pb.iter() {
        for &gid in gnodes {
            if !graph.gnodes().is_occupied(gid.index()) {
                continue;
            }
            let g = graph.gnodes().get(gid.index());
            if g.state() != GState::SemiInternal {
                continue;
            }

            let child = g.left().or_else(|| g.right());
            let Some(child_id) = child else {
                errors.push(format!(
                    "P-I4: semi-internal {gid:?} in plateau {key:?} has no children"
                ));
                continue;
            };

            let child_g = graph.gnodes().get(child_id.index());
            let child_lo = child_g.lo();

            let child_plateau_key = graph
                .plateaus
                .range(..=BasisEdge(child_lo))
                .next_back()
                .map(|(&k, _)| k);

            match child_plateau_key {
                Some(ck) if ck == key => {
                    errors.push(format!(
                        "P-I4 thatch one-hop: semi-internal {gid:?} in plateau {key:?} \
                         thatches child {child_id:?}, but child's plateau key {ck:?} == parent's"
                    ));
                }
                None => {
                    errors.push(format!(
                        "P-I4 thatch one-hop: semi-internal {gid:?} in plateau {key:?}: \
                         no plateau found covering child {child_id:?} at lo={child_lo:?}"
                    ));
                }
                Some(_) => {}
            }
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
pub fn check_p_i5_thatch_depth<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    errors: &mut Vec<String>,
) {
    let pb = graph.plateau_basis();

    let mut samples: Vec<C> = Vec::new();
    for (_, gnodes) in pb.iter() {
        for &gid in gnodes {
            if graph.gnodes().is_occupied(gid.index()) {
                samples.push(graph.gnodes().get(gid.index()).lo());
            }
        }
    }
    samples.sort_by(Coordinate::total_cmp);
    samples.dedup_by(|a, b| a.total_cmp(b) == std::cmp::Ordering::Equal);

    for x in &samples {
        let mut thatch_count = 0u32;
        for (&_key, gnodes) in pb.iter() {
            let covers = gnodes.iter().any(|&gid| {
                if !graph.gnodes().is_occupied(gid.index()) {
                    return false;
                }
                let g = graph.gnodes().get(gid.index());
                g.lo().total_cmp(x) != std::cmp::Ordering::Greater
                    && x.total_cmp(&g.hi()) == std::cmp::Ordering::Less
            });
            if covers {
                thatch_count += 1;
            }
        }

        let d_geo = route_to_depth(graph, *x);

        if thatch_count > d_geo + 1 {
            errors.push(format!(
                "P-I5 thatch depth: at x={x:?}, thatch_depth={thatch_count} > d_geo={d_geo} + 1"
            ));
        }
    }
}

#[cfg(feature = "dynamic-contour-tracking")]
fn route_to_depth<C: Coordinate, V: Accumulator + Inspectable, const N: u32>(
    graph: &GvGraph<C, V, N>,
    x: C,
) -> u32 {
    let mut cur = graph.g_root();
    for _ in 0..=N + 1 {
        let g = graph.gnodes().get(cur.index());
        if g.is_terminal() {
            return gnode_depth_from_interval(g.lo(), g.hi(), N);
        }
        let mid = C::midpoint(g.lo(), g.hi());
        let next = if x.total_cmp(&mid) == std::cmp::Ordering::Less {
            g.left()
        } else {
            g.right()
        };
        match next {
            Some(child) => cur = child,
            None => return gnode_depth_from_interval(g.lo(), g.hi(), N),
        }
    }
    let g = graph.gnodes().get(cur.index());
    gnode_depth_from_interval(g.lo(), g.hi(), N)
}
