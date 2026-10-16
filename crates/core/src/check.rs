//! Solver-independent checks of candidate points and LP duals.
//!
//! These functions trust no solver. They recompute objective values,
//! violations and dual residuals from the model data alone, in original,
//! unscaled units, so every backend (and every external reference solver) is
//! held to one definition. All checks are floating-point checks, not exact
//! certificates.

use crate::error::{Error, Result, ensure, ensure_finite};
use crate::model::{Model, Sense};
use crate::solver::{Duals, Evaluation, Tolerances};

/// Evaluates the point `x` of `model`: objective, row activities, the largest
/// bound violation, the largest integrality violation, and whether both are
/// within `tolerances`.
///
/// The point is used exactly as given: nothing is rounded, clamped or
/// repaired. It may lie outside the bounds.
///
/// Returns [`Error::InvalidInput`] for invalid tolerances or a point of the
/// wrong length or with non-finite entries, and [`Error::Evaluation`] if the
/// objective or a row activity overflows.
///
/// For integer variables beyond `2^52` in magnitude every `f64` is an integer,
/// so their integrality violation is always zero; such values need a model
/// change, not a tolerance.
pub fn evaluate(model: &Model, x: &[f64], tolerances: &Tolerances) -> Result<Evaluation> {
    tolerances.validate()?;
    check_point(model, x, "point")?;
    let row_activity = activity(model, x)?;
    let objective = model.objective_offset()
        + model
            .objective()
            .iter()
            .zip(x)
            .map(|(c, v)| c * v)
            .sum::<f64>();
    if !objective.is_finite() {
        return Err(Error::Evaluation(format!(
            "objective overflowed to {objective}"
        )));
    }
    let primal_violation = model
        .variables()
        .checked_violation(x)?
        .max(model.rows().checked_violation(&row_activity)?);
    let integrality_violation = model
        .kinds()
        .iter()
        .zip(x)
        .filter(|(kind, _)| kind.is_integral())
        .map(|(_, v)| (v - v.round()).abs())
        .fold(0.0_f64, f64::max);
    Ok(Evaluation {
        objective,
        row_activity,
        primal_violation,
        integrality_violation,
        feasible_candidate: primal_violation <= tolerances.feasibility
            && integrality_violation <= tolerances.integrality,
    })
}

/// First-order residuals of an LP point with duals, unscaled and `>= 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LpResiduals {
    /// Largest bound violation of variables and rows.
    pub primal: f64,
    /// `‖c - A^T row_duals - reduced_costs‖∞`.
    pub stationarity: f64,
    /// Largest `|μ| * |distance to the bound selected by the sign of μ|` over
    /// all row duals and reduced costs `μ`.
    pub complementarity: f64,
    /// Largest `|μ|` whose sign selects an infinite bound, for example a
    /// positive row dual of a minimization row without lower bound. Zero for
    /// sign-consistent duals.
    pub dual_sign: f64,
    /// Relative primal-dual objective gap `|P - D| / (1 + |P| + |D|)` with
    /// `P = c^T x` and `D = Σ μ · (bound selected by the sign of μ)` over all
    /// row duals and reduced costs with a finite selected bound (the others
    /// are counted in `dual_sign`). The objective offset cancels.
    pub duality_gap: f64,
}

impl LpResiduals {
    /// The largest of the five residuals.
    pub fn max(&self) -> f64 {
        self.primal
            .max(self.stationarity)
            .max(self.complementarity)
            .max(self.dual_sign)
            .max(self.duality_gap)
    }
}

/// Checks a primal point and duals in the [`Duals`] sign convention as a
/// solution of the LP, ignoring integrality (that is, of the LP relaxation of
/// a MIP).
///
/// All residuals are zero exactly at an optimal LP solution with consistent
/// duals; small values mean an approximately optimal pair. `complementarity`
/// is a product of a dual and a distance, so it grows with both magnitudes;
/// `duality_gap` is the scale-free measure of the same property. None of them
/// bounds the objective error, and they do not certify a MIP.
///
/// Returns [`Error::InvalidInput`] for wrong lengths or non-finite entries and
/// [`Error::Evaluation`] if any intermediate value overflows, so a NaN can
/// never vanish inside a maximum.
pub fn lp_residuals(model: &Model, x: &[f64], duals: &Duals) -> Result<LpResiduals> {
    check_point(model, x, "point")?;
    ensure(duals.row_duals.len() == model.m(), || {
        format!(
            "row duals: expected {} entries, got {}",
            model.m(),
            duals.row_duals.len()
        )
    })?;
    ensure(duals.reduced_costs.len() == model.n(), || {
        format!(
            "reduced costs: expected {} entries, got {}",
            model.n(),
            duals.reduced_costs.len()
        )
    })?;
    ensure_finite(&duals.row_duals, "row duals")?;
    ensure_finite(&duals.reduced_costs, "reduced costs")?;

    let row_activity = activity(model, x)?;
    let primal = model
        .variables()
        .checked_violation(x)?
        .max(model.rows().checked_violation(&row_activity)?);

    let mut aty = vec![0.0; model.n()];
    model
        .matrix()
        .multiply_transpose(&duals.row_duals, &mut aty);
    let mut stationarity = 0.0_f64;
    let mut primal_objective = 0.0_f64;
    for (j, ((&c, &a), &z)) in model
        .objective()
        .iter()
        .zip(&aty)
        .zip(&duals.reduced_costs)
        .enumerate()
    {
        finite(a, || format!("(A^T row_duals)[{j}]"))?;
        let residual = finite(c - a - z, || format!("stationarity of variable {j}"))?;
        stationarity = stationarity.max(residual.abs());
        primal_objective = finite(primal_objective + c * x[j], || "c^T x".into())?;
    }

    // Minimizing, a positive dual selects the lower bound; maximizing, the upper.
    let orientation = match model.sense() {
        Sense::Minimize => 1.0,
        Sense::Maximize => -1.0,
    };
    let mut complementarity = 0.0_f64;
    let mut dual_sign = 0.0_f64;
    let mut dual_objective = 0.0_f64;
    for (what, values, bounds, duals) in [
        ("row", &row_activity[..], model.rows(), &duals.row_duals[..]),
        ("variable", x, model.variables(), &duals.reduced_costs[..]),
    ] {
        for i in 0..values.len() {
            let (v, mu) = (values[i], duals[i]);
            let selected = match (orientation * mu).partial_cmp(&0.0) {
                Some(std::cmp::Ordering::Greater) => bounds.lower[i],
                Some(std::cmp::Ordering::Less) => bounds.upper[i],
                _ => continue,
            };
            if selected.is_finite() {
                let term = finite(mu.abs() * (v - selected).abs(), || {
                    format!("complementarity of {what} {i}")
                })?;
                complementarity = complementarity.max(term);
                dual_objective =
                    finite(dual_objective + mu * selected, || "dual objective".into())?;
            } else {
                dual_sign = dual_sign.max(mu.abs());
            }
        }
    }
    let duality_gap = finite(
        (primal_objective - dual_objective).abs()
            / (1.0 + primal_objective.abs() + dual_objective.abs()),
        || "duality gap".into(),
    )?;

    Ok(LpResiduals {
        primal,
        stationarity,
        complementarity,
        dual_sign,
        duality_gap,
    })
}

fn finite(value: f64, what: impl FnOnce() -> String) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Evaluation(format!(
            "{} overflowed to {value}",
            what()
        )))
    }
}

pub(crate) fn check_point(model: &Model, x: &[f64], what: &str) -> Result<()> {
    ensure(x.len() == model.n(), || {
        format!("{what}: expected {} entries, got {}", model.n(), x.len())
    })?;
    ensure_finite(x, what)
}

fn activity(model: &Model, x: &[f64]) -> Result<Vec<f64>> {
    let mut row_activity = vec![0.0; model.m()];
    model.matrix().multiply(x, &mut row_activity);
    match row_activity.iter().position(|v| !v.is_finite()) {
        Some(i) => Err(Error::Evaluation(format!(
            "activity of row {i} overflowed to {}",
            row_activity[i]
        ))),
        None => Ok(row_activity),
    }
}
