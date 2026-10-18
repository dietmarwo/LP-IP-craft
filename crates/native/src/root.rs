//! Root transformations. Matrix shape is frozen before the numerical engine.
use crate::*;
use lp_ip_craft::CsrMatrix;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Lower,
    Upper,
}
#[derive(Clone, Copy, Debug)]
struct BoundOwner {
    row: usize,
    col: usize,
    a: f64,
    side: Side,
}
struct Reduction {
    model: Model,
    rows: Vec<usize>,
    owners: Vec<BoundOwner>,
}

// Prove a*q=b in real arithmetic, including subnormal operands. Multiplication
// of two 53-bit significands fits u128; no rounded-product equality shortcut.
pub(crate) fn exact_product(a: f64, q: f64, b: f64) -> bool {
    fn parts(v: f64) -> (bool, u128, i32) {
        let bits = v.to_bits();
        let e = ((bits >> 52) & 2047) as i32;
        let mant = (bits & ((1u64 << 52) - 1)) | if e == 0 { 0 } else { 1u64 << 52 };
        (
            bits >> 63 != 0,
            mant as u128,
            if e == 0 { -1074 } else { e - 1075 },
        )
    }
    if !a.is_finite() || !q.is_finite() || !b.is_finite() {
        return false;
    }
    if a == 0. || q == 0. {
        return b == 0.;
    }
    if b == 0. {
        return false;
    }
    let (sa, ma, ea) = parts(a);
    let (sq, mq, eq) = parts(q);
    let (sb, mb, eb) = parts(b);
    let product = ma * mq;
    let shift = product.trailing_zeros();
    let bs = mb.trailing_zeros();
    sa ^ sq == sb && product >> shift == mb >> bs && ea + eq + shift as i32 == eb + bs as i32
}

fn singleton(model: &Model) -> Result<Option<Reduction>> {
    let mut bounds = model.variables().clone();
    let mut rows = Vec::with_capacity(model.m());
    let mut owners = Vec::new();
    let mut removed = 0;
    let a = model.matrix();
    for i in 0..model.m() {
        let mut entries =
            (a.pattern.indptr[i]..a.pattern.indptr[i + 1]).filter(|&k| a.values[k] != 0.);
        let k = entries.next();
        let single = k.is_some() && entries.next().is_none();
        if !single {
            rows.push(i);
            continue;
        }
        let k = k.unwrap();
        let c = a.values[k];
        let j = a.pattern.indices[k];
        let l = model.rows().lower[i];
        let u = model.rows().upper[i];
        if [l, u]
            .iter()
            .any(|&b| b.is_finite() && !exact_product(c, b / c, b))
        {
            rows.push(i);
            continue;
        }
        let (lo, hi) = if c > 0. {
            (l / c, u / c)
        } else {
            (u / c, l / c)
        };
        if lo > bounds.upper[j] || hi < bounds.lower[j] {
            return Ok(None);
        }
        if lo > bounds.lower[j] {
            bounds.lower[j] = lo;
            owners.push(BoundOwner {
                row: i,
                col: j,
                a: c,
                side: Side::Lower,
            });
        }
        if hi < bounds.upper[j] {
            bounds.upper[j] = hi;
            owners.push(BoundOwner {
                row: i,
                col: j,
                a: c,
                side: Side::Upper,
            });
        }
        removed += 1;
    }
    if removed == 0 {
        return Ok(None);
    }
    let mut data = model.data().clone();
    data.variables = bounds;
    let mut ptr = vec![0];
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for &i in &rows {
        let r = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        indices.extend_from_slice(&a.pattern.indices[r.clone()]);
        values.extend_from_slice(&a.values[r]);
        ptr.push(indices.len());
    }
    data.matrix = CsrMatrix::new(rows.len(), model.n(), ptr, indices, values);
    data.rows = Bounds::new(
        rows.iter().map(|&i| model.rows().lower[i]).collect(),
        rows.iter().map(|&i| model.rows().upper[i]).collect(),
    );
    Ok(Some(Reduction {
        model: Model::new(data)?,
        rows,
        owners,
    }))
}

fn restore(original: &Model, reduced: &Reduction, duals: &mut Duals) {
    let mut y = vec![0.; original.m()];
    for (&i, &v) in reduced.rows.iter().zip(&duals.row_duals) {
        y[i] = v;
    }
    let mut z = duals.reduced_costs.clone();
    for h in reduced.owners.iter().rev() {
        let s = oriented(original, z[h.col]);
        if (s > 0. && h.side == Side::Lower) || (s < 0. && h.side == Side::Upper) {
            y[h.row] += z[h.col] / h.a;
            z[h.col] = 0.;
        }
    }
    original.matrix().multiply_transpose(&y, &mut z);
    for (v, &c) in z.iter_mut().zip(original.objective()) {
        *v = c - *v;
    }
    duals.row_duals = y;
    duals.reduced_costs = z;
}

pub(crate) fn solve(
    solver: &mut NativeSolver,
    model: &Model,
    options: &Options,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
) -> Result<SolveResult> {
    options.validate()?;
    solver.settings.validate()?;
    if let Some(s) = start {
        if s.primal.len() != model.n() || s.primal.iter().any(|v| !v.is_finite()) {
            return Err(Error::InvalidInput(
                "start must contain n finite values".into(),
            ));
        }
    }
    if !presolve::supported_arithmetic()
        || !solver.settings.presolve
        || (!solver.settings.singleton_presolve
            && !solver.settings.root_propagation
            && !solver.settings.root_cuts
            && !solver.settings.root_separation
            && !solver.settings.mip_presolve
            && !solver.settings.coefficient_tightening
            && !solver.settings.dual_presolve
            && !solver.settings.gmi_cuts
            && !solver.settings.mir_cuts
            && !solver.settings.root_heuristics)
        || cancel.is_cancelled()
        || solver.settings.time_limit_seconds == Some(0.)
        || solver.settings.iteration_limit == Some(0)
        || solver.settings.node_limit == Some(0)
    {
        return solver.solve_internal(model, options, start, cancel, model);
    }
    let began = Instant::now();
    let reduction = if solver.settings.singleton_presolve {
        singleton(model)?
    } else {
        None
    };
    if reduction.is_none() && !model.is_mip() {
        return original_after_preprocessing(solver, model, options, start, cancel, began);
    }
    let mut reduced = reduction.unwrap_or_else(|| Reduction {
        model: model.clone(),
        rows: (0..model.m()).collect(),
        owners: Vec::new(),
    });
    let inferred = if solver.settings.root_propagation && model.is_mip() {
        infer_bounds(
            &mut reduced.model,
            cancel,
            began,
            solver.settings.time_limit_seconds,
        )?
    } else {
        0
    };
    let advanced =
        solver.settings.gmi_cuts || solver.settings.mir_cuts || solver.settings.root_heuristics;
    let presolve_stats = if model.is_mip()
        && (solver.settings.mip_presolve
            || solver.settings.coefficient_tightening
            || solver.settings.dual_presolve)
    {
        mip_presolve::run(&mut reduced.model, &solver.settings, options, cancel, began)?
    } else {
        mip_presolve::Stats::default()
    };
    let removed_rows = model.m() - reduced.model.m();
    let cuts = if solver.settings.root_cuts && model.is_mip() {
        cover_cuts(&mut reduced.model)?
    } else {
        0
    };
    let mut separation_pivots = 0u64;
    let mut separation_nodes = 0u64;
    let mut directed_cuts = 0usize;
    let mut lifted_cuts = 0usize;
    let mut cut_work = 0usize;
    let mut initial_bound = None;
    let mut initial_objective = None;
    if solver.settings.root_separation && model.is_mip() && start.is_none() && !advanced {
        for _ in 0..solver.settings.root_cut_rounds {
            if cancel.is_cancelled()
                || solver
                    .settings
                    .node_limit
                    .is_some_and(|n| separation_nodes >= n)
            {
                break;
            }
            let mut controls = solver.settings.clone();
            controls.decompose = false;
            controls.root_separation = false;
            if let Some(t) = &mut controls.time_limit_seconds {
                *t = (*t - began.elapsed().as_secs_f64()).max(0.);
            }
            if let Some(p) = &mut controls.iteration_limit {
                *p = p.saturating_sub(separation_pivots);
            }
            if controls.time_limit_seconds == Some(0.) || controls.iteration_limit == Some(0) {
                break;
            }
            let mut data = reduced.model.data().clone();
            data.kinds.fill(lp_ip_craft::VariableKind::Continuous);
            let relaxation = Model::new(data)?;
            let probe = NativeSolver::new(controls).solve_internal(
                &relaxation,
                options,
                None,
                cancel,
                &relaxation,
            )?;
            separation_pivots += probe.iterations;
            separation_nodes += 1;
            if initial_bound.is_none() {
                initial_bound = probe.best_bound;
                initial_objective = probe.evaluation.as_ref().map(|e| e.objective);
            }
            if probe.status != Status::Optimal {
                break;
            }
            let Some(x) = probe.candidate else {
                break;
            };
            let separated = cuts::separate(
                &mut reduced.model,
                &x,
                solver.settings.cover_lifting,
                cancel,
            )?;
            directed_cuts += separated.added;
            lifted_cuts += separated.lifted;
            cut_work += separated.work;
            if separated.added == 0 {
                break;
            }
        }
    }
    let advanced_stats = if advanced && model.is_mip() {
        rootcuts::run(
            &mut reduced.model,
            options,
            &solver.settings,
            start,
            cancel,
            began,
        )?
    } else {
        rootcuts::Stats::default()
    };
    separation_nodes += advanced_stats.nodes;
    separation_pivots += advanced_stats.pivots;
    let heuristic_start = advanced_stats
        .seed
        .as_ref()
        .map(|x| WarmStart::from_primal(x.clone()));
    let child_start = heuristic_start.as_ref().or(start);
    if reduced.model == *model
        && separation_nodes == 0
        && !advanced
        && !solver.settings.mip_presolve
        && !solver.settings.coefficient_tightening
        && !solver.settings.dual_presolve
    {
        return original_after_preprocessing(solver, model, options, start, cancel, began);
    }

    if let Some(s) = start {
        if check::evaluate(model, &s.primal, &options.tolerances)?.feasible_candidate
            && !check::evaluate(&reduced.model, &s.primal, &options.tolerances)?.feasible_candidate
        {
            return original_after_preprocessing(solver, model, options, start, cancel, began);
        }
    }
    let conversion = began.elapsed().as_secs_f64();
    let mut settings = solver.settings.clone();
    settings.singleton_presolve = false;
    settings.root_propagation = false;
    settings.root_cuts = false;
    settings.root_separation = false;
    settings.mip_presolve = false;
    settings.coefficient_tightening = false;
    settings.dual_presolve = false;
    settings.gmi_cuts = false;
    settings.mir_cuts = false;
    settings.root_heuristics = false;
    if let Some(n) = &mut settings.node_limit {
        *n = n.saturating_sub(separation_nodes);
    }
    if let Some(p) = &mut settings.iteration_limit {
        *p = p.saturating_sub(separation_pivots);
    }
    if let Some(t) = &mut settings.time_limit_seconds {
        *t = (*t - conversion).max(0.);
    }
    let mut result = NativeSolver::new(settings).solve_internal(
        &reduced.model,
        options,
        child_start,
        cancel,
        model,
    )?;
    // A checked root bound of the cut model also bounds every limited or
    // failed child search; it says nothing about an infeasible or unbounded one.
    if let Some(b) = advanced_stats.bound.filter(|_| {
        !matches!(
            result.status,
            Status::Infeasible | Status::Unbounded | Status::InfeasibleOrUnbounded
        )
    }) {
        result.best_bound = Some(result.best_bound.map_or(b, |old| {
            if model.sense() == Sense::Minimize {
                old.max(b)
            } else {
                old.min(b)
            }
        }));
    }
    result.nodes += separation_nodes;
    result.iterations += separation_pivots;
    for (key, value) in [
        ("mip_presolve_rounds", presolve_stats.rounds),
        ("mip_presolve_bounds", presolve_stats.bounds),
        ("mip_presolve_redundant", presolve_stats.redundant),
        ("mip_presolve_forcing", presolve_stats.forcing),
        ("mip_presolve_coefficients", presolve_stats.coefficients),
        ("mip_presolve_dual_fixed", presolve_stats.dual_fixed),
        ("mip_presolve_exact_work", presolve_stats.work),
        ("mip_presolve_conflicts", presolve_stats.conflicts),
        ("gmi_cuts", advanced_stats.gmi),
        ("mir_cuts", advanced_stats.mir),
        ("root_basis_reuses", advanced_stats.basis_reuses),
        ("root_cuts_pruned", advanced_stats.pruned),
        ("cut_exact_work", advanced_stats.exact_work),
        ("cut_candidates", advanced_stats.candidates),
        ("cut_rejected", advanced_stats.rejected),
        ("mir_vubs", advanced_stats.vubs),
        ("root_heuristic_lp_calls", advanced_stats.primal.calls),
        (
            "root_heuristic_candidates",
            advanced_stats.primal.candidates,
        ),
        ("root_heuristic_cycles", advanced_stats.primal.cycles),
        ("root_heuristic_failures", advanced_stats.primal.failures),
    ] {
        result
            .backend
            .effective_options
            .push((format!("diagnostics.{key}"), value.to_string()));
    }
    result.backend.effective_options.push((
        "diagnostics.root_stages".into(),
        format!("[{}]", advanced_stats.stages.join(",")),
    ));
    let number = |v: Option<f64>| v.map_or_else(|| "none".to_string(), |v| v.to_string());
    for (key, value) in [
        ("directed_root_cuts", directed_cuts.to_string()),
        ("lifted_root_cuts", lifted_cuts.to_string()),
        ("root_cut_work", cut_work.to_string()),
        ("root_separation_nodes", separation_nodes.to_string()),
        ("root_separation_pivots", separation_pivots.to_string()),
        ("root_separation_initial_bound", number(initial_bound)),
        (
            "root_separation_initial_objective",
            number(initial_objective),
        ),
    ] {
        result
            .backend
            .effective_options
            .push((format!("diagnostics.{key}"), value));
    }
    if solver.settings.mip_diagnostics && model.is_mip() {
        result.backend.effective_options.push((
            "diagnostics.mip_search.preprocessing_seconds".into(),
            conversion.to_string(),
        ));
    }
    let checks = Instant::now();
    result
        .backend
        .effective_options
        .push(("root_cover_cuts".into(), cuts.to_string()));
    result
        .backend
        .effective_options
        .push(("root_inferred_bounds".into(), inferred.to_string()));
    if let Some(x) = &result.candidate {
        result.evaluation = Some(check::evaluate(model, x, &options.tolerances)?);
        let mut valid = result.evaluation.as_ref().unwrap().feasible_candidate;
        if let Some(d) = &mut result.duals {
            restore(model, &reduced, d);
            valid &= accepted(model, model.variables(), x, d, options, &solver.settings);
            result.best_bound =
                safe_dual_bound_with_bounds(model, model.variables(), &d.row_duals)?.bound;
        }
        if !valid {
            result.status = Status::NumericalFailure;
            result.message = "root postsolve rejected by original-model checks".into();
            result.best_bound = None;
        }
        if solver.settings.mip_diagnostics && model.is_mip() {
            result.backend.effective_options.push((
                "diagnostics.mip_search.final_original_check".into(),
                valid.to_string(),
            ));
        }
    }
    for (key, value) in effective_options(&solver.settings) {
        if let Some(v) = result
            .backend
            .effective_options
            .iter_mut()
            .find(|(k, _)| k == &key)
        {
            v.1 = value;
        }
    }
    result
        .backend
        .effective_options
        .push(("singleton_removed_rows".into(), removed_rows.to_string()));
    result.backend.conversion_seconds += conversion;
    result.backend.check_seconds += checks.elapsed().as_secs_f64();
    result.backend.total_seconds = began.elapsed().as_secs_f64();
    result.backend.time_budget_exceeded = solver
        .settings
        .time_limit_seconds
        .is_some_and(|t| result.backend.total_seconds > t);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::{ModelData, VariableKind};
    #[test]
    fn product_proof_includes_subnormal_and_rejects_rounded_equality() {
        for (a, q, b) in [
            (2., 0.5, 1.),
            (-4., 0.25, -1.),
            (f64::from_bits(1), 2., f64::from_bits(2)),
            (1e200, 1e-200, 1.),
        ] {
            if a == 1e200 {
                assert!(!exact_product(a, q, b));
            } else {
                assert!(exact_product(a, q, b));
            }
        }
        assert_eq!(3. * (1. / 3.), 1.);
        assert!(!exact_product(3., 1. / 3., 1.));
        assert!(!exact_product(f64::MAX, 2., f64::INFINITY));
        assert!(!exact_product(f64::from_bits(1), 0.5, 0.));
    }
    fn fixture(sense: Sense, integral: bool) -> Model {
        Model::new(ModelData {
            sense,
            objective: vec![if sense == Sense::Minimize { -1. } else { 1. }, 0.25],
            objective_offset: 3.,
            variables: Bounds::new(vec![0., 0.], vec![10., 10.]),
            kinds: vec![
                if integral {
                    VariableKind::Integer
                } else {
                    VariableKind::Continuous
                };
                2
            ],
            matrix: CsrMatrix::from_dense(5, 2, &[2., 0., -4., 0., 1., 0., 0., -2., 1., 1.])
                .unwrap(),
            rows: Bounds::new(
                vec![f64::NEG_INFINITY, -16., 1., -4., f64::NEG_INFINITY],
                vec![6., f64::INFINITY, 3., 0., 4.],
            ),
        })
        .unwrap()
    }
    #[test]
    fn singleton_postsolve_restores_both_senses_and_bound_owners() {
        for sense in [Sense::Minimize, Sense::Maximize] {
            for integer in [false, true] {
                let m = fixture(sense, integer);
                let red = singleton(&m).unwrap().unwrap();
                assert_eq!(red.model.m(), 1);
                let r = NativeSolver::default()
                    .solve(&m, &Options::default(), None, &CancellationToken::default())
                    .unwrap();
                assert_eq!(r.status, Status::Optimal, "{r:?}");
                assert!(r.evaluation.as_ref().unwrap().feasible_candidate);
                if !integer {
                    assert!(
                        check::lp_residuals(
                            &m,
                            r.candidate.as_ref().unwrap(),
                            r.duals.as_ref().unwrap()
                        )
                        .unwrap()
                        .max()
                            < 1e-7
                    );
                }
            }
        }
    }
    #[test]
    fn inexact_singleton_is_kept_and_zero_budgets_bypass() {
        let mut d = fixture(Sense::Minimize, false).into_data();
        d.matrix.values[0] = 3.;
        d.rows.upper[0] = 1.;
        let m = Model::new(d).unwrap();
        let red = singleton(&m).unwrap().unwrap();
        assert!(red.rows.contains(&0));
        let r = NativeSolver::new(NativeOptions {
            iteration_limit: Some(0),
            ..Default::default()
        })
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
        assert_eq!(r.status, Status::IterationLimit);
        assert!(
            !r.backend
                .effective_options
                .iter()
                .any(|(k, _)| k == "singleton_removed_rows")
        );
    }
}

// Even an unproductive presolve scan consumes the whole-call wall budget.
fn original_after_preprocessing(
    solver: &NativeSolver,
    model: &Model,
    options: &Options,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    began: Instant,
) -> Result<SolveResult> {
    let conversion = began.elapsed().as_secs_f64();
    let mut settings = solver.settings.clone();
    if let Some(t) = &mut settings.time_limit_seconds {
        *t = (*t - conversion).max(0.);
    }
    let mut r = NativeSolver::new(settings).solve_internal(model, options, start, cancel, model)?;
    if solver.settings.mip_diagnostics && model.is_mip() {
        r.backend.effective_options.push((
            "diagnostics.mip_search.preprocessing_seconds".into(),
            conversion.to_string(),
        ));
    }
    for (k, v) in effective_options(&solver.settings) {
        if let Some(p) = r.backend.effective_options.iter_mut().find(|p| p.0 == k) {
            p.1 = v;
        }
    }
    r.backend.conversion_seconds += conversion;
    r.backend.total_seconds = began.elapsed().as_secs_f64();
    r.backend.time_budget_exceeded = solver
        .settings
        .time_limit_seconds
        .is_some_and(|t| r.backend.total_seconds > t);
    Ok(r)
}

fn down(v: f64) -> f64 {
    if v == 0. {
        -f64::from_bits(1)
    } else if v > 0. {
        f64::from_bits(v.to_bits() - 1)
    } else {
        f64::from_bits(v.to_bits() + 1)
    }
}
fn up(v: f64) -> f64 {
    -down(-v)
}
pub(crate) fn quotient(a: f64, b: f64, lower: bool) -> Option<f64> {
    let q = b / a;
    if !q.is_finite() {
        return None;
    }
    let v = if exact_product(a, q, b) {
        q
    } else if lower {
        down(q)
    } else {
        up(q)
    };
    v.is_finite().then_some(v)
}
// At most four sweeps. Totals are from a wider snapshot of the row box, so
// subsequent tightenings within the sweep cannot invalidate another inference.
fn infer_bounds(
    model: &mut Model,
    cancel: &CancellationToken,
    began: Instant,
    limit: Option<f64>,
) -> Result<usize> {
    let mut bounds = model.variables().clone();
    let mut changed = 0usize;
    let a = model.matrix();
    for _ in 0..4 {
        let before = changed;
        for i in 0..model.m() {
            if i % 128 == 0
                && (cancel.is_cancelled()
                    || limit.is_some_and(|t| began.elapsed().as_secs_f64() >= t))
            {
                break;
            }
            let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
            // Existing exact node propagation already covers finite pure integer rows.
            if range.clone().all(|k| {
                let j = a.pattern.indices[k];
                model.kinds()[j].is_integral()
                    && a.values[k].fract() == 0.
                    && bounds.lower[j].is_finite()
                    && bounds.upper[j].is_finite()
            }) {
                continue;
            }
            let mut terms = Vec::with_capacity(range.len());
            let (mut ni, mut xi) = (0, 0);
            let mut unsafe_row = false;
            for k in range.clone() {
                let j = a.pattern.indices[k];
                let c = a.values[k];
                if c == 0. {
                    terms.push((Some((0., 0.)), Some((0., 0.))));
                    continue;
                }
                let (l, u) = if c > 0. {
                    (bounds.lower[j], bounds.upper[j])
                } else {
                    (bounds.upper[j], bounds.lower[j])
                };
                let min = if l.is_finite() {
                    incumbent::dot(std::iter::once((c, l)), 0.)
                } else {
                    ni += 1;
                    None
                };
                let max = if u.is_finite() {
                    incumbent::dot(std::iter::once((c, u)), 0.)
                } else {
                    xi += 1;
                    None
                };
                unsafe_row |= (l.is_finite() && min.is_none()) || (u.is_finite() && max.is_none());
                terms.push((min, max));
            }
            if unsafe_row {
                continue;
            }
            let totalmin = incumbent::dot(terms.iter().filter_map(|t| t.0.map(|v| (1., v.0))), 0.);
            let totalmax = incumbent::dot(terms.iter().filter_map(|t| t.1.map(|v| (1., v.1))), 0.);
            let (Some(totalmin), Some(totalmax)) = (totalmin, totalmax) else {
                continue;
            };
            for (pos, k) in range.enumerate() {
                let c = a.values[k];
                if c == 0. {
                    continue;
                }
                let j = a.pattern.indices[k];
                let (mn, mx) = terms[pos];
                let restmin = if ni == 1 && mn.is_none() {
                    Some(totalmin.0)
                } else if ni == 0 {
                    incumbent::dot(std::iter::once((-1., mn.unwrap().1)), totalmin.0).map(|v| v.0)
                } else {
                    None
                };
                let restmax = if xi == 1 && mx.is_none() {
                    Some(totalmax.1)
                } else if xi == 0 {
                    incumbent::dot(std::iter::once((-1., mx.unwrap().0)), totalmax.1).map(|v| v.1)
                } else {
                    None
                };
                let mut lo = bounds.lower[j];
                let mut hi = bounds.upper[j];
                if let Some(rest) = restmin {
                    let b = model.rows().upper[i];
                    if b.is_finite() {
                        if let Some((_, rhs)) = incumbent::dot(std::iter::once((-1., rest)), b) {
                            if let Some(v) = quotient(c, rhs, c < 0.) {
                                if c > 0. {
                                    hi = hi.min(v);
                                } else {
                                    lo = lo.max(v);
                                }
                            }
                        }
                    }
                }
                if let Some(rest) = restmax {
                    let b = model.rows().lower[i];
                    if b.is_finite() {
                        if let Some((rhs, _)) = incumbent::dot(std::iter::once((-1., rest)), b) {
                            if let Some(v) = quotient(c, rhs, c > 0.) {
                                if c > 0. {
                                    lo = lo.max(v);
                                } else {
                                    hi = hi.min(v);
                                }
                            }
                        }
                    }
                }
                if model.kinds()[j].is_integral() {
                    lo = lo.ceil();
                    hi = hi.floor();
                }
                if lo > hi {
                    return Ok(0);
                } // leave infeasibility classification to the engine
                if lo > bounds.lower[j] {
                    bounds.lower[j] = lo;
                    changed += 1;
                }
                if hi < bounds.upper[j] {
                    bounds.upper[j] = hi;
                    changed += 1;
                }
            }
        }
        if changed == before {
            break;
        }
    }
    if changed > 0 {
        let mut d = model.data().clone();
        d.variables = bounds;
        *model = Model::new(d)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod propagation_tests {
    use super::*;
    use lp_ip_craft::{ModelData, VariableKind};
    #[test]
    fn mixed_infinite_root_domains_are_inferred_before_admission() {
        for sense in [Sense::Minimize, Sense::Maximize] {
            let m = Model::new(ModelData {
                sense,
                objective: vec![1., 0.5],
                objective_offset: 0.,
                variables: Bounds::new(vec![f64::NEG_INFINITY, 0.], vec![f64::INFINITY; 2]),
                kinds: vec![VariableKind::Integer, VariableKind::Continuous],
                matrix: CsrMatrix::from_dense(3, 2, &[1., 1., -1., 1., 1., -1.]).unwrap(),
                rows: Bounds::new(
                    vec![f64::NEG_INFINITY, f64::NEG_INFINITY, -2.],
                    vec![5., 3., f64::INFINITY],
                ),
            })
            .unwrap();
            let mut inferred = m.clone();
            assert!(
                infer_bounds(
                    &mut inferred,
                    &CancellationToken::default(),
                    Instant::now(),
                    None
                )
                .unwrap()
                    > 0
            );
            assert!(
                inferred
                    .variables()
                    .lower
                    .iter()
                    .chain(&inferred.variables().upper)
                    .all(|v| v.is_finite())
            );
            let r = NativeSolver::default()
                .solve(&m, &Options::default(), None, &CancellationToken::default())
                .unwrap();
            assert_eq!(r.status, Status::Optimal, "{r:?}");
            assert!(r.evaluation.unwrap().feasible_candidate);
            assert!(
                NativeSolver::new(NativeOptions {
                    root_propagation: false,
                    ..Default::default()
                })
                .solve(&m, &Options::default(), None, &CancellationToken::default())
                .is_err()
            );
        }
    }
    #[test]
    fn outward_inferences_preserve_exhaustive_dyadic_points() {
        for seed in 1..80usize {
            let c = (seed % 7 + 1) as f64 / 4.;
            let d = -((seed % 5 + 1) as f64) / 8.;
            let m = Model::new(ModelData {
                sense: Sense::Minimize,
                objective: vec![1., 0.],
                objective_offset: 0.,
                variables: Bounds::new(vec![-4.; 2], vec![4.; 2]),
                kinds: vec![VariableKind::Integer, VariableKind::Continuous],
                matrix: CsrMatrix::from_dense(1, 2, &[c, d]).unwrap(),
                rows: Bounds::new(vec![-1.25], vec![2.75]),
            })
            .unwrap();
            let mut inferred = m.clone();
            infer_bounds(
                &mut inferred,
                &CancellationToken::default(),
                Instant::now(),
                None,
            )
            .unwrap();
            for x in -4..=4 {
                for yi in -32..=32 {
                    let y = yi as f64 / 8.;
                    let activity = c * x as f64 + d * y;
                    if (-1.25..=2.75).contains(&activity) {
                        assert!(
                            inferred.variables().lower[0] <= x as f64
                                && inferred.variables().upper[0] >= x as f64
                        );
                        assert!(
                            inferred.variables().lower[1] <= y
                                && inferred.variables().upper[1] >= y
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn unresolved_free_domains_and_overflow_do_not_get_invented_bounds() {
        let m = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![1.; 2],
            objective_offset: 0.,
            variables: Bounds::free(2),
            kinds: vec![VariableKind::Integer; 2],
            matrix: CsrMatrix::from_dense(1, 2, &[1., -1.]).unwrap(),
            rows: Bounds::fixed(vec![0.]),
        })
        .unwrap();
        let mut copy = m.clone();
        assert_eq!(
            infer_bounds(
                &mut copy,
                &CancellationToken::default(),
                Instant::now(),
                None
            )
            .unwrap(),
            0
        );
        assert_eq!(m, copy);
        assert!(quotient(1e-300, 1e300, true).is_none());
    }
}

#[derive(Clone, Debug)]
struct Cover {
    row: usize,
    cols: Vec<usize>,
}
fn verify_cover(m: &Model, cover: &Cover) -> bool {
    if cover.row >= m.m() || cover.cols.len() < 2 || cover.cols.windows(2).any(|p| p[0] >= p[1]) {
        return false;
    }
    let b = m.rows().upper[cover.row];
    if !b.is_finite() || !(0. ..=2_147_483_647.).contains(&b) {
        return false;
    }
    let a = m.matrix();
    let mut sum = 0i128;
    let mut found = 0;
    for k in a.pattern.indptr[cover.row]..a.pattern.indptr[cover.row + 1] {
        let c = a.values[k];
        let j = a.pattern.indices[k];
        if c == 0. {
            continue;
        }
        if !m.kinds()[j].is_integral()
            || m.variables().lower[j] < 0.
            || m.variables().upper[j] > 1.
            || c.fract() != 0.
            || !(1. ..=2_147_483_647.).contains(&c)
        {
            return false;
        }
        if cover.cols.binary_search(&j).is_ok() {
            sum += c as i128;
            found += 1;
        }
    }
    found == cover.cols.len() && sum > b.floor() as i128
}
fn cover_cuts(model: &mut Model) -> Result<usize> {
    let a = model.matrix();
    let mut covers = Vec::<Cover>::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut work = 0;
    for i in 0..model.m() {
        let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        if !(3..=512).contains(&range.len()) {
            continue;
        }
        work += range.len();
        if work > 100_000 || covers.len() >= 8 {
            break;
        }
        let all: Vec<_> = range
            .clone()
            .filter(|&k| a.values[k] != 0.)
            .map(|k| a.pattern.indices[k])
            .collect();
        if !verify_cover(model, &Cover { row: i, cols: all }) {
            continue;
        }
        let weights: Vec<_> = range
            .filter(|&k| a.values[k] != 0.)
            .map(|k| (a.pattern.indices[k], a.values[k] as i128))
            .collect();
        for order in 0..3 {
            let mut sorted = weights.clone();
            sorted.sort_by(|&(j, w), &(k, v)| match order {
                0 => (oriented(model, model.objective()[j]) / w as f64)
                    .total_cmp(&(oriented(model, model.objective()[k]) / v as f64))
                    .then(j.cmp(&k)),
                1 => v.cmp(&w).then(j.cmp(&k)),
                _ => w.cmp(&v).then(j.cmp(&k)),
            });
            let capacity = model.rows().upper[i].floor() as i128;
            let mut sum = 0i128;
            let mut chosen = Vec::new();
            for pair in sorted {
                sum += pair.1;
                chosen.push(pair);
                if sum > capacity {
                    break;
                }
            }
            for pos in (0..chosen.len()).rev() {
                if sum - chosen[pos].1 > capacity {
                    sum -= chosen.remove(pos).1;
                }
            }
            let mut cols: Vec<_> = chosen.iter().map(|t| t.0).collect();
            cols.sort_unstable();
            let witness = Cover { row: i, cols };
            if verify_cover(model, &witness) && seen.insert(witness.cols.clone()) {
                covers.push(witness);
            }
            if covers.len() >= 8 {
                break;
            }
        }
    }
    if covers.is_empty() {
        return Ok(0);
    }
    let mut data = model.data().clone();
    for c in &covers {
        data.matrix.pattern.indices.extend_from_slice(&c.cols);
        data.matrix
            .values
            .extend(std::iter::repeat_n(1., c.cols.len()));
        data.matrix.pattern.indptr.push(data.matrix.values.len());
        data.matrix.pattern.rows += 1;
        data.rows.lower.push(f64::NEG_INFINITY);
        data.rows.upper.push((c.cols.len() - 1) as f64);
    }
    *model = Model::new(data)?;
    Ok(covers.len())
}

#[cfg(test)]
mod cut_tests {
    use super::*;
    use lp_ip_craft::{ModelData, VariableKind};
    #[test]
    fn exact_cover_witnesses_and_exhaustive_both_sense_optima() {
        for seed in 0..24 {
            for sense in [Sense::Minimize, Sense::Maximize] {
                let n = 8;
                let coeff: Vec<_> = (0..n)
                    .map(|j| ((j * 7 + seed * 3) % 13 + 2) as f64)
                    .collect();
                let cap = coeff.iter().sum::<f64>() * 0.5;
                let m = Model::new(ModelData {
                    sense,
                    objective: (0..n)
                        .map(|j| {
                            ((j * 11 + seed) % 17 + 1) as f64
                                * if sense == Sense::Minimize { -1. } else { 1. }
                        })
                        .collect(),
                    objective_offset: 2.,
                    variables: Bounds::new(vec![0.; n], vec![1.; n]),
                    kinds: vec![VariableKind::Binary; n],
                    matrix: CsrMatrix::from_dense(1, n, &coeff).unwrap(),
                    rows: Bounds::new(vec![f64::NEG_INFINITY], vec![cap]),
                })
                .unwrap();
                let mut cut = m.clone();
                assert!(cover_cuts(&mut cut).unwrap() > 0);
                let mut best = f64::INFINITY;
                for mask in 0..1usize << n {
                    let x: Vec<_> = (0..n).map(|j| ((mask >> j) & 1) as f64).collect();
                    if check::evaluate(&m, &x, &Options::default().tolerances)
                        .unwrap()
                        .feasible_candidate
                    {
                        assert!(
                            check::evaluate(&cut, &x, &Options::default().tolerances)
                                .unwrap()
                                .feasible_candidate
                        );
                        let obj = m.objective_offset()
                            + m.objective()
                                .iter()
                                .zip(&x)
                                .map(|(a, b)| a * b)
                                .sum::<f64>();
                        best = best.min(oriented(&m, obj));
                    }
                }
                let r = NativeSolver::new(NativeOptions {
                    root_cuts: true,
                    ..Default::default()
                })
                .solve(&m, &Options::default(), None, &CancellationToken::default())
                .unwrap();
                assert_eq!(r.status, Status::Optimal, "{r:?}");
                assert!((oriented(&m, r.evaluation.unwrap().objective) - best).abs() < 1e-7);
                assert!(!verify_cover(
                    &m,
                    &Cover {
                        row: 0,
                        cols: vec![0, 0]
                    }
                ));
                assert!(!verify_cover(
                    &m,
                    &Cover {
                        row: 0,
                        cols: vec![0, 999]
                    }
                ));
            }
        }
    }
}
