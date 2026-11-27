//! Integer consequences of short equality rows with finite integral domains.
use crate::incumbent::{add, product};
use lp_ip_craft::{Model, Tolerances};

pub(crate) type Terms = Vec<(usize, i128)>;
pub(crate) type Residue = (i128, i128);

/// Return an exact equation in the rounded integer coordinates. An integer
/// row residual with absolute value strictly below one must be zero. The
/// enclosed error includes the source row and integrality tolerances and
/// the coefficient and RHS conversions over the entire admissible box.
pub(crate) fn normalized(
    model: &Model,
    row: usize,
    tolerances: &Tolerances,
) -> Option<(Terms, i128)> {
    let rhs = model.rows().lower[row];
    if !rhs.is_finite() || rhs != model.rows().upper[row] {
        return None;
    }
    let a = model.matrix();
    let range = a.pattern.indptr[row]..a.pattern.indptr[row + 1];
    if !(2..=12).contains(&range.len())
        || range.clone().any(|k| {
            let j = a.pattern.indices[k];
            a.values[k] != 0.
                && (!model.kinds()[j].is_integral()
                    || model.variables().lower[j].abs() > 2_147_483_648.
                    || model.variables().upper[j].abs() > 2_147_483_648.)
        })
    {
        return None;
    }
    [1., 10., 100.].into_iter().find_map(|scale| {
        let qrhs = (scale * rhs).round();
        if qrhs.abs() > 1_125_899_906_842_624. {
            return None;
        }
        let residual = |value: f64, rounded: f64| -> Option<f64> {
            let (lo, hi) = product(scale, value)?;
            Some(add(lo, -rounded)?.0.abs().max(add(hi, -rounded)?.1.abs()))
        };
        let mut error = add(
            product(scale, tolerances.feasibility)?.1,
            residual(rhs, qrhs)?,
        )?
        .1;
        let mut terms = Vec::with_capacity(range.len());
        for k in range.clone().filter(|&k| a.values[k] != 0.) {
            let (j, c) = (a.pattern.indices[k], a.values[k]);
            let q = (scale * c).round();
            if q.abs() > 2_147_483_648. {
                return None;
            }
            let max = model.variables().lower[j]
                .abs()
                .max(model.variables().upper[j].abs());
            let radius = add(add(max, tolerances.feasibility)?.1, tolerances.integrality)?.1;
            let scaled_c = product(scale, c.abs())?.1;
            let integer_error = product(scaled_c, tolerances.integrality)?.1;
            // Allow rounding in the original short activity calculation as
            // well as the mathematical residual. The guard also bounds rhs
            // subtraction near cancellation.
            let fp_margin = (4. * (range.len() as f64 + 2.)) * f64::EPSILON;
            let activity_error = product(product(scaled_c, radius)?.1, fp_margin)?.1;
            error = add(error, product(radius, residual(c, q)?)?.1)?.1;
            error = add(error, integer_error)?.1;
            error = add(error, activity_error)?.1;
            if error >= 1. {
                return None;
            }
            if q != 0. {
                terms.push((j, q as i128));
            }
        }
        (!terms.is_empty() && error < 1.).then_some((terms, qrhs as i128))
    })
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Other coefficients' gcd divides their contribution irrespective of the
/// current node. Keep only short residue searches. The inferred interval is
/// applied again after a branch or any other propagation narrows the box.
pub(crate) fn residues(terms: &[(usize, i128)], rhs: i128) -> Vec<Option<Residue>> {
    let result: Vec<_> = terms
        .iter()
        .enumerate()
        .map(|(i, &(_, a))| {
            let d = terms
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .fold(0, |g, (_, &(_, coefficient))| gcd(g, coefficient.abs()));
            if !(2..=32).contains(&d) {
                return None;
            }
            let step = d / gcd(a.abs(), d);
            if step == 1 {
                return None;
            }
            (0..step)
                .find(|&r| (a * r - rhs).rem_euclid(d) == 0)
                .map(|r| (r, step))
        })
        .collect();
    if result.iter().any(Option::is_some) {
        result
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::{Bounds, CsrMatrix, ModelData, Sense, VariableKind};

    fn fixture(coefficients: &[f64], kinds: Vec<VariableKind>, bound: f64) -> Model {
        Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.; 3],
            objective_offset: 0.,
            variables: Bounds::new(vec![-bound; 3], vec![bound; 3]),
            kinds,
            matrix: CsrMatrix::from_dense(1, 3, coefficients).unwrap(),
            rows: Bounds::new(vec![0.], vec![0.]),
        })
        .unwrap()
    }

    #[test]
    fn bounded_decimal_equation_preserves_enumerated_integer_points() {
        for a in [0.9, -0.9] {
            let m = fixture(&[a, 1., -1.], vec![VariableKind::Integer; 3], 12.);
            let tol = Tolerances::default();
            let (terms, rhs) = normalized(&m, 0, &tol).unwrap();
            assert_eq!(terms, [(0, (10. * a).round() as i128), (1, 10), (2, -10)]);
            assert_eq!(residues(&terms, rhs)[0], Some((0, 10)));
            for x in -12_i128..=12 {
                for y in -12_i128..=12 {
                    for z in -12_i128..=12 {
                        if (a * x as f64 + y as f64 - z as f64).abs() <= tol.feasibility {
                            assert_eq!(x.rem_euclid(10), 0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn skips_unverified_scalings_and_live_continuous_columns() {
        let tol = Tolerances::default();
        for (c, limit) in [(0.904, 100.), (0.90000001, 1e9)] {
            let m = fixture(&[c, 1., -1.], vec![VariableKind::Integer; 3], limit);
            assert!(normalized(&m, 0, &tol).is_none());
        }
        let mut kinds = vec![VariableKind::Integer; 3];
        kinds[1] = VariableKind::Continuous;
        let m = fixture(&[0.9, 1., -1.], kinds, 12.);
        assert!(normalized(&m, 0, &tol).is_none());
    }
}
