//! Conservative reductions over the original box; no approximate dropping.
use lp_ip_craft::{Bounds, Model};

pub(crate) fn supported_arithmetic() -> bool {
    use std::hint::black_box as bb;
    let tiny = bb(f64::from_bits(1));
    (bb(f64::MIN_POSITIVE) * bb(0.5)).to_bits() == 1_u64 << 51
        && (tiny + tiny).to_bits() == 2
        && (tiny * bb(2.)).to_bits() == 2
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
