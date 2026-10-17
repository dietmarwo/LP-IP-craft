//! Conservative integer-domain propagation; no floating-point activity sums.
use crate::Budget;
use lp_ip_craft::{Bounds, Model};

struct Row {
    terms: Vec<(usize, i128)>,
    lower: Option<i128>,
    upper: Option<i128>,
}
pub(crate) struct Domain {
    integers: Vec<usize>,
    rows: Vec<Row>,
}
fn floor_div(a: i128, b: i128) -> i128 {
    debug_assert!(b > 0);
    a.div_euclid(b)
}
fn ceil_div(a: i128, b: i128) -> i128 {
    -floor_div(-a, b)
}
impl Domain {
    pub fn new(model: &Model, enabled: bool) -> Self {
        let mut domain = Self {
            integers: Vec::new(),
            rows: Vec::new(),
        };
        if !enabled || !model.is_mip() {
            return domain;
        }
        domain.integers = (0..model.n())
            .filter(|&j| {
                model.kinds()[j].is_integral()
                    && (model.variables().lower[j].fract() != 0.
                        || model.variables().upper[j].fract() != 0.)
            })
            .collect();
        let a = model.matrix();
        for i in 0..model.m() {
            let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
            // Short rows capture implications cheaply; leave long knapsack
            // propagation to the LP until an incremental activity engine exists.
            if range.len() > 8 {
                continue;
            }
            // At most usize::MAX terms, each at most 2^62: sum < 2^126
            // on a 64-bit target, safely inside signed i128.
            if !range.clone().all(|k| {
                let (j, v) = (a.pattern.indices[k], a.values[k]);
                v == 0.
                    || (model.kinds()[j].is_integral()
                        && v.fract() == 0.
                        && v.abs() <= 2_147_483_648.
                        && model.variables().lower[j].abs() <= 2_147_483_648.
                        && model.variables().upper[j].abs() <= 2_147_483_648.)
            }) {
                continue;
            }
            let lower = model.rows().lower[i].ceil();
            let upper = model.rows().upper[i].floor();
            let endpoint =
                |v: f64| (v.is_finite() && v.abs() <= 1_125_899_906_842_624.).then_some(v as i128);
            if endpoint(lower).is_none() && endpoint(upper).is_none() {
                continue;
            }
            domain.rows.push(Row {
                terms: range
                    .filter(|&k| a.values[k] != 0.)
                    .map(|k| (a.pattern.indices[k], a.values[k] as i128))
                    .collect(),
                lower: endpoint(lower),
                upper: endpoint(upper),
            });
        }
        domain
    }
    /// Every appended column must be restored when leaving this node.
    pub fn apply(
        &self,
        bounds: &mut Bounds,
        changed: &mut Vec<usize>,
        budget: &Budget<'_>,
    ) -> bool {
        if changed.is_empty() && self.integers.is_empty() {
            return true;
        }
        for &j in &self.integers {
            let (l, u) = (bounds.lower[j].ceil(), bounds.upper[j].floor());
            if l > u {
                return false;
            }
            if l != bounds.lower[j] || u != bounds.upper[j] {
                bounds.lower[j] = l;
                bounds.upper[j] = u;
                changed.push(j);
            }
        }
        for _ in 0..3 {
            let before = changed.len();
            for (row_no, row) in self.rows.iter().enumerate() {
                if row_no % 256 == 0 && budget.stop().is_some() {
                    return true;
                }
                let (mut min, mut max) = (0i128, 0i128);
                for &(j, a) in &row.terms {
                    let (l, u) = (bounds.lower[j] as i128, bounds.upper[j] as i128);
                    min += a * if a > 0 { l } else { u };
                    max += a * if a > 0 { u } else { l };
                }
                if row.lower.is_some_and(|l| max < l) || row.upper.is_some_and(|u| min > u) {
                    return false;
                }
                // Each CSR column occurs once in this row. Its bounds have
                // not yet changed, so the cached old activity remains a valid
                // (possibly weaker) enclosure for the other variables.
                for &(j, a) in &row.terms {
                    let (l, u) = (bounds.lower[j] as i128, bounds.upper[j] as i128);
                    let width = a.abs() * (u - l);
                    let amin = a * if a > 0 { l } else { u };
                    let amax = a * if a > 0 { u } else { l };
                    let (mut lo, mut hi) = (l, u);
                    if a > 0 {
                        if let Some(b) = row.upper.filter(|&b| b - min < width) {
                            hi = hi.min(floor_div(b - (min - amin), a));
                        }
                        if let Some(b) = row.lower.filter(|&b| max - b < width) {
                            lo = lo.max(ceil_div(b - (max - amax), a));
                        }
                    } else {
                        if let Some(b) = row.upper.filter(|&b| b - min < width) {
                            lo = lo.max(ceil_div((min - amin) - b, -a));
                        }
                        if let Some(b) = row.lower.filter(|&b| max - b < width) {
                            hi = hi.min(floor_div((max - amax) - b, -a));
                        }
                    }
                    if lo > hi {
                        return false;
                    }
                    if lo != l || hi != u {
                        bounds.lower[j] = lo as f64;
                        bounds.upper[j] = hi as f64;
                        changed.push(j);
                    }
                }
            }
            if changed.len() == before {
                break;
            }
        }
        true
    }
}
