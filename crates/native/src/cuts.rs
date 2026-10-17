//! LP-directed binary knapsack covers with exact bounded sequential lifting.
use crate::CancellationToken;
use lp_ip_craft::{Model, Result};
#[derive(Default)]
pub(crate) struct Stats {
    pub added: usize,
    pub lifted: usize,
    pub work: usize,
}
/// Exact minimum weight for every attainable cut profit. None means effort cap.
fn max_profit(terms: &[(usize, i128, i64)], capacity: i128, work: &mut usize) -> Option<i64> {
    if capacity < 0 {
        return None;
    }
    let total: usize = terms.iter().map(|t| t.2 as usize).sum();
    if total > 8192 {
        return None;
    }
    let mut dp = vec![i128::MAX; total + 1];
    dp[0] = 0;
    let mut end = 0;
    for &(_, w, c) in terms {
        let c = c as usize;
        if c == 0 {
            continue;
        }
        for p in (0..=end).rev() {
            *work += 1;
            if *work > 2_000_000 {
                return None;
            }
            if dp[p] != i128::MAX {
                let v = dp[p] + w;
                if v <= capacity {
                    dp[p + c] = dp[p + c].min(v);
                }
            }
        }
        end += c;
    }
    (0..dp.len())
        .rev()
        .find(|&p| dp[p] <= capacity)
        .map(|p| p as i64)
}
pub(crate) fn separate(
    model: &mut Model,
    x: &[f64],
    lifting: bool,
    cancel: &CancellationToken,
) -> Result<Stats> {
    let mut stats = Stats::default();
    let a = model.matrix();
    let mut output: Vec<(Vec<(usize, i64)>, i64)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for i in 0..model.m() {
        if cancel.is_cancelled() || stats.work > 2_000_000 || output.len() >= 8 {
            break;
        }
        let b = model.rows().upper[i];
        let r = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        if !(3..=128).contains(&r.len()) || !b.is_finite() || !(0. ..=2_147_483_647.).contains(&b) {
            continue;
        }
        let mut weights = Vec::new();
        let mut valid = true;
        for k in r {
            let (j, v) = (a.pattern.indices[k], a.values[k]);
            if v == 0. {
                continue;
            }
            if !model.kinds()[j].is_integral()
                || model.variables().lower[j] < 0.
                || model.variables().upper[j] > 1.
                || v.fract() != 0.
                || !(1. ..=2_147_483_647.).contains(&v)
            {
                valid = false;
                break;
            }
            weights.push((j, v as i128));
        }
        if !valid {
            continue;
        }
        let capacity = b.floor() as i128;
        for order in 0..2 {
            let mut sorted = weights.clone();
            sorted.sort_by(|&(j, w), &(k, v)| {
                if order == 0 {
                    x[k].total_cmp(&x[j]).then(j.cmp(&k))
                } else {
                    ((1. - x[j]) / w as f64)
                        .total_cmp(&((1. - x[k]) / v as f64))
                        .then(j.cmp(&k))
                }
            });
            let mut sum = 0;
            let mut cover = Vec::new();
            for pair in sorted {
                sum += pair.1;
                cover.push(pair);
                if sum > capacity {
                    break;
                }
            }
            if sum <= capacity {
                continue;
            }
            for p in (0..cover.len()).rev() {
                if sum - cover[p].1 > capacity {
                    sum -= cover.remove(p).1;
                }
            }
            if cover.len() < 2 {
                continue;
            }
            let rhs = (cover.len() - 1) as i64;
            let mut terms: Vec<_> = cover.iter().map(|&(j, w)| (j, w, 1i64)).collect();
            if lifting {
                let mut outside: Vec<_> = weights
                    .iter()
                    .copied()
                    .filter(|(j, _)| !cover.iter().any(|t| t.0 == *j))
                    .collect();
                outside.sort_by(|a, b| x[b.0].total_cmp(&x[a.0]).then(a.0.cmp(&b.0)));
                for (j, w) in outside {
                    if cancel.is_cancelled() {
                        break;
                    }
                    if w > capacity {
                        // The row alone forces x_j = 0; leave it out of the cut
                        // and keep lifting the remaining items.
                        continue;
                    }
                    if let Some(max) = max_profit(&terms, capacity - w, &mut stats.work) {
                        let alpha = (rhs - max).max(0);
                        if alpha > 0 {
                            terms.push((j, w, alpha));
                        }
                    } else {
                        break;
                    }
                }
            }
            // Independent final exact knapsack validation. Unlifted cover proof also checked.
            if max_profit(&terms, capacity, &mut stats.work).is_none_or(|v| v > rhs) {
                continue;
            }
            let mut cut: Vec<_> = terms.iter().map(|&(j, _, c)| (j, c)).collect();
            cut.sort_unstable();
            let activity: f64 = cut.iter().map(|&(j, c)| x[j] * c as f64).sum();
            if activity <= rhs as f64 + 1e-7 {
                continue;
            }
            let duplicate = (0..model.m()).any(|row| {
                let r = a.pattern.indptr[row]..a.pattern.indptr[row + 1];
                model.rows().upper[row] == rhs as f64
                    && r.len() == cut.len()
                    && r.zip(&cut)
                        .all(|(k, &(j, c))| a.pattern.indices[k] == j && a.values[k] == c as f64)
            });
            if duplicate || !seen.insert(cut.clone()) {
                continue;
            }
            stats.lifted += usize::from(terms.len() > cover.len());
            output.push((cut, rhs));
            if output.len() >= 8 {
                break;
            }
        }
    }
    if !output.is_empty() {
        let mut data = model.data().clone();
        for (cut, rhs) in &output {
            for &(j, c) in cut {
                data.matrix.pattern.indices.push(j);
                data.matrix.values.push(c as f64);
            }
            data.matrix.pattern.indptr.push(data.matrix.values.len());
            data.matrix.pattern.rows += 1;
            data.rows.lower.push(f64::NEG_INFINITY);
            data.rows.upper.push(*rhs as f64);
        }
        stats.added = output.len();
        *model = Model::new(data)?;
    }
    Ok(stats)
}
#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::*;
    #[test]
    fn directed_lifted_cover_is_violated_and_exhaustively_valid() {
        let mut m = Model::new(ModelData {
            sense: Sense::Maximize,
            objective: vec![1.; 4],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; 4], vec![1.; 4]),
            kinds: vec![VariableKind::Binary; 4],
            matrix: CsrMatrix::from_dense(1, 4, &[3., 3., 3., 4.]).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY], vec![5.]),
        })
        .unwrap();
        let original = m.clone();
        let stats = separate(
            &mut m,
            &[0.55, 0.55, 0.55, 0.],
            true,
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(stats.added > 0 && stats.lifted > 0);
        for mask in 0..16 {
            let x: Vec<_> = (0..4).map(|j| ((mask >> j) & 1) as f64).collect();
            if check::evaluate(&original, &x, &Tolerances::default())
                .unwrap()
                .feasible_candidate
            {
                assert!(
                    check::evaluate(&m, &x, &Tolerances::default())
                        .unwrap()
                        .feasible_candidate
                );
            }
        }
    }
    #[test]
    fn items_heavier_than_the_capacity_do_not_stop_lifting() {
        // x3 alone exceeds the capacity; x4 must still be lifted after it.
        let mut m = Model::new(ModelData {
            sense: Sense::Maximize,
            objective: vec![1.; 5],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; 5], vec![1.; 5]),
            kinds: vec![VariableKind::Binary; 5],
            matrix: CsrMatrix::from_dense(1, 5, &[3., 3., 3., 9., 4.]).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY], vec![5.]),
        })
        .unwrap();
        let original = m.clone();
        let x = [0.55, 0.55, 0.55, 0., 0.];
        let stats = separate(&mut m, &x, true, &CancellationToken::default()).unwrap();
        assert!(stats.added > 0);
        let a = m.matrix();
        let lifted_x4 = (1..m.m()).any(|i| {
            (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
                .any(|k| a.pattern.indices[k] == 4 && a.values[k] > 0.)
        });
        assert!(lifted_x4, "x4 should be lifted past the infeasible item x3");
        for mask in 0..32 {
            let p: Vec<_> = (0..5).map(|j| ((mask >> j) & 1) as f64).collect();
            if check::evaluate(&original, &p, &Tolerances::default())
                .unwrap()
                .feasible_candidate
            {
                assert!(
                    check::evaluate(&m, &p, &Tolerances::default())
                        .unwrap()
                        .feasible_candidate
                );
            }
        }
    }
    #[test]
    fn exact_profit_matches_enumeration() {
        let mut seed = 3u64;
        for n in 2..10 {
            for _ in 0..30 {
                let mut terms = Vec::new();
                for j in 0..n {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    terms.push((
                        j,
                        ((seed >> 33) % 13 + 1) as i128,
                        ((seed >> 28) % 5 + 1) as i64,
                    ));
                }
                let cap = 20;
                let exact = (0..1usize << n)
                    .filter_map(|mask| {
                        let (w, p) = terms
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| (mask >> j) & 1 != 0)
                            .fold((0, 0), |(w, p), (_, t)| (w + t.1, p + t.2));
                        (w <= cap).then_some(p)
                    })
                    .max()
                    .unwrap();
                assert_eq!(max_profit(&terms, cap, &mut 0), Some(exact));
            }
        }
    }
}
