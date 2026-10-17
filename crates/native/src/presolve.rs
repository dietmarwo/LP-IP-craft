//! Conservative reductions over the original box; no approximate dropping.
use crate::NativeOptions;
use lp_ip_craft::{Bounds, Model};

pub(crate) fn supported_arithmetic() -> bool {
    use std::hint::black_box as bb;
    let tiny = bb(f64::from_bits(1));
    (bb(f64::MIN_POSITIVE) * bb(0.5)).to_bits() == 1_u64 << 51
        && (tiny + tiny).to_bits() == 2
        && (tiny * bb(2.)).to_bits() == 2
}

/// Rows the LP keeps. With presolve, free rows and feasible empty rows are
/// dropped exactly; with `redundant_rows` also rows whose activity over the
/// root box is conservatively inside their bounds (then over every node box).
pub(crate) fn kept_rows(model: &Model, bounds: &Bounds, settings: &NativeOptions) -> Vec<bool> {
    if !settings.presolve {
        return vec![true; model.m()];
    }
    let scan = settings.redundant_rows
        && model.m() as f64 >= settings.redundant_row_min_ratio * model.n() as f64;
    let mut keep = active_rows(model, bounds, scan);
    let matrix = model.matrix();
    for (i, kept) in keep.iter_mut().enumerate() {
        let (l, u) = (model.rows().lower[i], model.rows().upper[i]);
        let empty = (matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1])
            .all(|k| matrix.values[k] == 0.);
        if (l == f64::NEG_INFINITY && u == f64::INFINITY) || (empty && l <= 0. && u >= 0.) {
            *kept = false;
        }
    }
    keep
}

/// Columns the LP keeps: with presolve, columns fixed by their root bounds
/// are substituted into the rows.
pub(crate) fn kept_columns(model: &Model, bounds: &Bounds, settings: &NativeOptions) -> Vec<bool> {
    (0..model.n())
        .map(|j| !settings.presolve || bounds.lower[j] != bounds.upper[j])
        .collect()
}

pub(crate) fn active_rows(model: &Model, bounds: &Bounds, enabled: bool) -> Vec<bool> {
    if !enabled || !supported_arithmetic() {
        return vec![true; model.m()];
    }
    let matrix = model.matrix();
    (0..model.m())
        .map(|i| {
            let (l, u) = (model.rows().lower[i], model.rows().upper[i]);
            if l == f64::NEG_INFINITY && u == f64::INFINITY {
                return false;
            }
            let range = matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1];
            let terms = |lower: bool| {
                range.clone().filter_map(move |k| {
                    let a = matrix.values[k];
                    if a == 0. {
                        return None;
                    }
                    let j = matrix.pattern.indices[k];
                    let b = if (a > 0.) == lower {
                        bounds.lower[j]
                    } else {
                        bounds.upper[j]
                    };
                    Some((a, b))
                })
            };
            let lower = if l.is_finite() {
                crate::incumbent::dot(terms(true), 0.).map(|v| v.0)
            } else {
                Some(f64::NEG_INFINITY)
            };
            let upper = if u.is_finite() {
                crate::incumbent::dot(terms(false), 0.).map(|v| v.1)
            } else {
                Some(f64::INFINITY)
            };
            !(lower.is_some_and(|v| v >= l) && upper.is_some_and(|v| v <= u))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::*;
    #[test]
    fn only_enclosed_rows_are_removed() {
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![1., 2.],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; 2], vec![1.; 2]),
            kinds: vec![VariableKind::Continuous; 2],
            matrix: CsrMatrix::from_dense(4, 2, &[1., 1., 1., 1., 0.1, 0.2, 0., 0.]).unwrap(),
            rows: Bounds::new(vec![0., 0., 0., 0.], vec![2., 1.999999999999, 0.3, 0.]),
        })
        .unwrap();
        assert_eq!(
            active_rows(&model, model.variables(), true),
            vec![false, true, true, false]
        );
        assert_eq!(active_rows(&model, model.variables(), false), vec![true; 4]);
        assert!(active_rows(&model, &Bounds::free(2), true)[0]);
    }
}

/// Integer-valued row activities permit exact inward rounding of bounds.
/// This is a MIP-equivalent relaxation, not an LP-equivalent reduction.
pub(crate) fn strengthen_integer_rows(model: &Model) -> lp_ip_craft::Result<Option<Model>> {
    if !model.is_mip() {
        return Ok(None);
    }
    let a = model.matrix();
    let mut data = None;
    for i in 0..model.m() {
        let integral = (a.pattern.indptr[i]..a.pattern.indptr[i + 1]).all(|k| {
            let v = a.values[k];
            v == 0.
                || (model.kinds()[a.pattern.indices[k]].is_integral()
                    && v.fract() == 0.
                    && v.abs() <= 9_007_199_254_740_991.)
        });
        if !integral {
            continue;
        }
        let (l, u) = (model.rows().lower[i].ceil(), model.rows().upper[i].floor());
        if l <= u && (l != model.rows().lower[i] || u != model.rows().upper[i]) {
            let d = data.get_or_insert_with(|| model.data().clone());
            d.rows.lower[i] = l;
            d.rows.upper[i] = u;
        }
    }
    data.map(Model::new).transpose()
}

#[cfg(test)]
mod integer_tests {
    use super::*;
    use lp_ip_craft::*;
    #[test]
    fn row_rounding_requires_exact_integral_activity() {
        let m = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![1., 1., 1.],
            objective_offset: 0.,
            variables: Bounds::new(vec![-2.; 3], vec![2.; 3]),
            kinds: vec![
                VariableKind::Integer,
                VariableKind::Integer,
                VariableKind::Continuous,
            ],
            matrix: CsrMatrix::from_dense(
                4,
                3,
                &[-1., 2., 0., 0., 1., 1., 0.5, 1., 0., 1., 0., 0.],
            )
            .unwrap(),
            rows: Bounds::new(vec![-1.7, -1.7, -1.7, 0.2], vec![2.9, 2.9, 2.9, 0.8]),
        })
        .unwrap();
        let r = strengthen_integer_rows(&m).unwrap().unwrap();
        assert_eq!(r.rows().lower, vec![-1., -1.7, -1.7, 0.2]);
        assert_eq!(r.rows().upper, vec![2., 2.9, 2.9, 0.8]);
        assert_eq!(m.rows().upper[0], 2.9);
    }
}
