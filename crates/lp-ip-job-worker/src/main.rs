//! LP/MIP artifact adapter for the common optimization job protocol.
#![forbid(unsafe_code)]
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, Error, Gap, LpIpSolver, Model, ModelData, Options, Sense,
    SolveResult, Status, Tolerances, VariableKind, WarmStart, check,
};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
use optimization_job_api::{
    JobSpec, Result,
    result::{Evidence, ObjectiveBounds, Outcome, objectives_agree},
    worker::{self, Prepared, TaskResult, Worker},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// The engine version: this crate shares the workspace version with
/// `lp-ip-craft-native`, so the reported version cannot drift from the
/// linked solver.
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROFILE: &str = "lp-ip.solve/1";
const EVALUATE_PROFILE: &str = "lp-ip.evaluate/1";
const MODEL_SCHEMA: &str = "lp-ip.model/1";
/// Largest numerical width of the native kernels (`Options::threads`).
const INNER_THREADS_MAX: usize = 8;
/// Discovery data of `describe` (JSON Schema draft 2020-12 and runnable
/// examples), kept as files so that the Python tests and agents can read
/// them without the binary.
const SOLVE_PAYLOAD_SCHEMA: &str = include_str!("../discovery/lp-ip.solve-1.schema.json");
const EVALUATE_PAYLOAD_SCHEMA: &str = include_str!("../discovery/lp-ip.evaluate-1.schema.json");
const MODEL_INPUT_SCHEMA: &str = include_str!("../discovery/lp-ip.model-1.schema.json");
const EXAMPLES: &str = include_str!("../discovery/examples.json");

/// An embedded discovery file; the tests check that each one parses.
fn embedded(text: &str) -> Value {
    serde_json::from_str(text).expect("embedded discovery JSON is valid")
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Minimize,
    Maximize,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Continuous,
    Integer,
    Binary,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sparse {
    indptr: Vec<usize>,
    indices: Vec<usize>,
    values: Vec<f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    schema_version: String,
    sense: Direction,
    objective: Vec<f64>,
    objective_offset: f64,
    variable_lower: Vec<Option<f64>>,
    variable_upper: Vec<Option<f64>>,
    kinds: Vec<Kind>,
    matrix: Sparse,
    row_lower: Vec<Option<f64>>,
    row_upper: Vec<Option<f64>>,
}
fn bounds(l: Vec<Option<f64>>, u: Vec<Option<f64>>) -> Bounds {
    Bounds::new(
        l.into_iter()
            .map(|v| v.unwrap_or(f64::NEG_INFINITY))
            .collect(),
        u.into_iter().map(|v| v.unwrap_or(f64::INFINITY)).collect(),
    )
}
impl ModelFile {
    fn model(self) -> Result<Model> {
        if self.schema_version != MODEL_SCHEMA {
            return Err("unsupported LP model schema".into());
        }
        let n = self.objective.len();
        let m = self.row_lower.len();
        Ok(Model::new(ModelData {
            objective: self.objective,
            objective_offset: self.objective_offset,
            sense: match self.sense {
                Direction::Minimize => Sense::Minimize,
                Direction::Maximize => Sense::Maximize,
            },
            variables: bounds(self.variable_lower, self.variable_upper),
            kinds: self
                .kinds
                .into_iter()
                .map(|k| match k {
                    Kind::Continuous => VariableKind::Continuous,
                    Kind::Integer => VariableKind::Integer,
                    Kind::Binary => VariableKind::Binary,
                })
                .collect(),
            matrix: CsrMatrix::new(
                m,
                n,
                self.matrix.indptr,
                self.matrix.indices,
                self.matrix.values,
            ),
            rows: bounds(self.row_lower, self.row_upper),
        })?)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    model_input: String,
    starts: Vec<Option<Vec<f64>>>,
    node_limit: u64,
    iteration_limit: u64,
    feasibility_tolerance: f64,
    integrality_tolerance: f64,
    presolve: bool,
}
/// `lp-ip.evaluate/1`: points to check against the model, without solving.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluatePayload {
    model_input: String,
    points: Vec<Vec<f64>>,
    feasibility_tolerance: f64,
    integrality_tolerance: f64,
}
impl Payload {
    /// Native settings for one task, apart from its remaining time.
    ///
    /// `presolve` is native's master switch: when it is off, the root skips
    /// every reduction (singleton and MIP presolve, coefficient tightening,
    /// bound propagation) and nodes skip domain propagation. Those controls
    /// are cleared too, so the reported effective options say what ran.
    /// Component, packing-neighborhood and tree workers are serial: only
    /// `Options::threads` may use the task's inner budget.
    fn settings(&self) -> NativeOptions {
        let defaults = NativeOptions::default();
        NativeOptions {
            node_limit: Some(self.node_limit),
            iteration_limit: Some(self.iteration_limit),
            presolve: self.presolve,
            singleton_presolve: self.presolve && defaults.singleton_presolve,
            root_propagation: self.presolve && defaults.root_propagation,
            mip_presolve: self.presolve && defaults.mip_presolve,
            coefficient_tightening: self.presolve && defaults.coefficient_tightening,
            component_threads: 1,
            packing_workers: 1,
            tree_threads: 1,
            ..defaults
        }
    }
}
struct Adapter;
struct Run {
    model: Model,
    payload: Payload,
    settings: NativeOptions,
    options: Options,
    cancel: CancellationToken,
}
struct Evaluate {
    model: Model,
    points: Vec<Vec<f64>>,
    tolerances: Tolerances,
}
/// Loads the single SHA-256 checked model input named `name`.
fn load(s: &JobSpec, base: &Path, name: &str) -> Result<Model> {
    if s.inputs.len() != 1 || s.inputs[0].name != name {
        return Err("exactly one input, named by model_input, is required".into());
    }
    serde_json::from_slice::<ModelFile>(&s.input_bytes(base, name)?)?.model()
}
impl Worker for Adapter {
    fn describe(&self) -> Value {
        let mut d = worker::description(
            "lp-ip-craft",
            ENGINE_VERSION,
            json!([PROFILE, EVALUATE_PROFILE]),
            json!([{"id":MODEL_SCHEMA,"input":"SHA-256 checked sparse CSR JSON artifact","warm_start":"complete MIP primal candidates only"}]),
            INNER_THREADS_MAX,
        );
        d["cancellation"] = json!("native cooperative token plus runner process deadline");
        d["terminations"] = json!(
            "native statuses (optimal, infeasible, unbounded, infeasible_or_unbounded, *_limit, cancelled, ...), unsupported for a model outside the native domain, evaluated for lp-ip.evaluate/1"
        );
        d["claims"] = json!({
            "numerical_optimum":"native optimal status; the candidate is re-evaluated (objective, bounds, constraints, integrality) and an LP's duals pass the KKT acceptance; numerical, not an exact certificate",
            "feasible":"a re-evaluated candidate within the tolerances, without an optimality claim",
            "infeasible":"native infeasibility claim (claimed; checked internally with a Farkas row, which is not exported); refuted by a feasible returned point or start",
            "unbounded":"native unboundedness claim (claimed); a returned point is only a feasible point",
            "infeasible_or_unbounded":"native claim that one of the two holds (claimed)",
            "none":"no claim; evidence.refuted names an optimal status that failed its re-check"
        });
        d["kkt_acceptance"] = json!(
            "LP with duals: primal <= feasibility_tolerance, stationarity and dual_sign <= dual_tolerance * (1 + max|c|), duality_gap <= dual_tolerance (native defaults)"
        );
        d["evaluation"] = json!({"profile":EVALUATE_PROFILE,"inner_threads":1,
            "points":"1 or more finite points of the model's dimension (inline: the resolved job, points included, must stay below the runner's 4 MiB validation output); one task per point; a point whose values overflow is reported with claim none",
            "result":"claim feasible within the tolerances, else none; objective, violations and objective_bounds.primal for a feasible point"});
        // Enough for a client without a checkout to build and validate one
        // job per profile.
        d["payload_schemas"] = json!({
            PROFILE: embedded(SOLVE_PAYLOAD_SCHEMA),
            EVALUATE_PROFILE: embedded(EVALUATE_PAYLOAD_SCHEMA),
        });
        d["input_schemas"] = json!({MODEL_SCHEMA: embedded(MODEL_INPUT_SCHEMA)});
        d["examples"] = embedded(EXAMPLES);
        d
    }
    fn prepare(&self, s: &mut JobSpec, base: &Path) -> Result<Box<dyn Prepared>> {
        if s.payload_schema == EVALUATE_PROFILE {
            return prepare_evaluate(s, base);
        }
        if s.payload_schema != PROFILE {
            return Err(format!(
                "unsupported payload schema; this worker accepts {PROFILE} and {EVALUATE_PROFILE}"
            )
            .into());
        }
        if s.resources.inner_threads > INNER_THREADS_MAX {
            return Err(format!("{PROFILE} supports inner_threads=1..{INNER_THREADS_MAX}").into());
        }
        let p: Payload = serde_json::from_value(s.payload.clone())?;
        if p.starts.is_empty()
            || p.starts.len() > 1_000_000
            || p.node_limit == 0
            || p.iteration_limit == 0
        {
            return Err("nonempty starts and positive node/iteration limits required".into());
        }
        let model = load(s, base, &p.model_input)?;
        for x in p.starts.iter().flatten() {
            if !model.is_mip() {
                return Err("LP warm starts are unsupported; use null".into());
            }
            if x.len() != model.n() || x.iter().any(|v| !v.is_finite()) {
                return Err("invalid MIP start dimension or value".into());
            }
        }
        let options = Options {
            threads: s.resources.inner_threads,
            tolerances: Tolerances {
                feasibility: p.feasibility_tolerance,
                integrality: p.integrality_tolerance,
            },
        };
        options.validate()?;
        // Reject settings native would refuse now, not in every task.
        let settings = p.settings();
        NativeOptions {
            time_limit_seconds: Some(s.limits.wall_time_seconds),
            ..settings.clone()
        }
        .validate()?;
        s.payload = serde_json::to_value(&p)?;
        Ok(Box::new(Run {
            model,
            payload: p,
            settings,
            options,
            cancel: CancellationToken::default(),
        }))
    }
}
fn prepare_evaluate(s: &mut JobSpec, base: &Path) -> Result<Box<dyn Prepared>> {
    if s.resources.inner_threads != 1 {
        return Err(format!("{EVALUATE_PROFILE} evaluates serially; use inner_threads=1").into());
    }
    let p: EvaluatePayload = serde_json::from_value(s.payload.clone())?;
    let model = load(s, base, &p.model_input)?;
    let tolerances = Tolerances {
        feasibility: p.feasibility_tolerance,
        integrality: p.integrality_tolerance,
    };
    tolerances.validate()?;
    if p.points.is_empty() || p.points.len() > 1_000_000 {
        return Err("points must hold 1..1000000 points".into());
    }
    for (i, x) in p.points.iter().enumerate() {
        if x.len() != model.n() || x.iter().any(|v| !v.is_finite()) {
            return Err(format!(
                "point {i} needs {} finite entries, one per variable",
                model.n()
            )
            .into());
        }
    }
    s.payload = serde_json::to_value(&p)?;
    Ok(Box::new(Evaluate {
        model,
        points: p.points,
        tolerances,
    }))
}
fn scalar(x: f64) -> Value {
    if x.is_finite() {
        json!(x)
    } else if x.is_sign_positive() {
        json!("+infinity")
    } else {
        json!("-infinity")
    }
}
fn work(nodes: u64, iterations: u64, validations: u64) -> BTreeMap<String, u64> {
    [
        ("nodes".into(), nodes),
        ("simplex_iterations".into(), iterations),
        ("validation_evaluations".into(), validations),
    ]
    .into()
}
/// What re-evaluating a point checks: objective, variable bounds, row
/// bounds and, for a MIP, integrality.
fn point_checks(model: &Model) -> Vec<&'static str> {
    let mut checks = vec!["objective", "bounds", "constraints"];
    if model.is_mip() {
        checks.push("integrality");
    }
    checks
}
fn objective_bounds(
    model: &Model,
    primal: Option<f64>,
    dual: Option<f64>,
) -> Option<ObjectiveBounds> {
    let dual = dual.filter(|v| v.is_finite());
    (primal.is_some() || dual.is_some()).then(|| ObjectiveBounds {
        sense: model.sense().as_str().into(),
        primal,
        dual,
    })
}
/// The native acceptance of an LP point with duals, recomputed in original
/// units: what makes `numerical_optimum` hold for an LP.
fn kkt_accepted(model: &Model, k: &check::LpResiduals, tolerances: &Tolerances, dual: f64) -> bool {
    let scale = 1.0
        + model
            .objective()
            .iter()
            .fold(0.0_f64, |a, &b| a.max(b.abs()));
    k.primal <= tolerances.feasibility
        && k.dual_sign <= dual * scale
        && k.stationarity <= dual * scale
        && k.duality_gap <= dual
}
impl Run {
    /// Judges a native result: claim, evidence and objective bounds. A
    /// status whose independent check fails is refuted, never reported:
    /// optimal with an infeasible point (`infeasible_point`), an LP optimum
    /// whose duals fail the KKT acceptance (`false_optimality`), or an
    /// infeasibility claim contradicted by a feasible returned point or start
    /// (`false_infeasibility`).
    fn judge(&self, task: usize, r: SolveResult) -> Result<TaskResult> {
        let tolerances = &self.options.tolerances;
        let checks = point_checks(&self.model);
        let checked = r
            .candidate
            .as_ref()
            .map(|x| check::evaluate(&self.model, x, tolerances))
            .transpose()?;
        let mut validations = u64::from(checked.is_some());
        let feasible = checked.as_ref().map(|e| e.feasible_candidate);
        // Residuals that overflow cannot confirm optimality; they fail the
        // acceptance instead of the task.
        let residuals = match (&r.candidate, &r.duals) {
            (Some(x), Some(d)) => {
                Some(check::lp_residuals(&self.model, x, d).map_err(|e| e.to_string()))
            }
            _ => None,
        };
        let kkt = residuals.as_ref().map(|k| {
            k.as_ref().is_ok_and(|k| {
                kkt_accepted(&self.model, k, tolerances, self.settings.dual_tolerance)
            })
        });
        // The point the outcome reports, with its evaluation.
        let mut x = r.candidate.clone();
        let mut shown = checked.clone();
        let mut shown_residuals = residuals.as_ref().and_then(|k| k.as_ref().ok().copied());
        let mut note = None;
        let recomputed: &[&str] = if checked.is_some() { &checks } else { &[] };
        // The solver's own objective for its candidate, when the
        // recomputation disagrees with it.
        let mismatch = match (&r.evaluation, &checked) {
            (Some(own), Some(e)) if !objectives_agree(own.objective, e.objective) => {
                Some(own.objective)
            }
            _ => None,
        };
        let (claim, evidence) = match r.status {
            Status::Optimal if checked.is_none() => {
                note = Some("optimal status without a candidate: nothing to check");
                ("none", Evidence::claimed(&[]))
            }
            Status::Optimal if feasible != Some(true) => {
                let e = checked.as_ref().expect("checked candidate");
                let message = format!(
                    "primal violation {} (tolerance {}), integrality violation {} (tolerance {})",
                    e.primal_violation,
                    tolerances.feasibility,
                    e.integrality_violation,
                    tolerances.integrality
                );
                (
                    "none",
                    Evidence::rechecked(&checks).refuting(
                        "numerical_optimum",
                        "infeasible_point",
                        message,
                    ),
                )
            }
            Status::Optimal if kkt == Some(false) => {
                let message = match residuals.as_ref().expect("KKT judged from residuals") {
                    Ok(k) => format!(
                        "primal {}, stationarity {}, dual_sign {}, duality_gap {} fail the acceptance (dual tolerance {})",
                        k.primal,
                        k.stationarity,
                        k.dual_sign,
                        k.duality_gap,
                        self.settings.dual_tolerance
                    ),
                    Err(e) => format!("the dual residuals cannot be computed: {e}"),
                };
                let mut all = checks.clone();
                all.push("kkt");
                (
                    "feasible",
                    Evidence::rechecked(&all).refuting(
                        "numerical_optimum",
                        "false_optimality",
                        message,
                    ),
                )
            }
            // Without KKT residuals that confirm the point, an optimum whose
            // own objective its point does not evaluate to is not believed.
            Status::Optimal if kkt.is_none() && mismatch.is_some() => {
                let reported = mismatch.expect("guarded");
                (
                    "feasible",
                    Evidence::rechecked(&checks).refuting(
                        "numerical_optimum",
                        "objective_mismatch",
                        format!(
                            "the solver reports objective {reported}, its point evaluates to {}",
                            checked.as_ref().expect("checked candidate").objective
                        ),
                    ),
                )
            }
            Status::Optimal => {
                let mut all = checks.clone();
                if kkt.is_some() {
                    all.push("kkt");
                }
                ("numerical_optimum", Evidence::rechecked(&all))
            }
            Status::Infeasible if feasible == Some(true) => (
                "feasible",
                Evidence::rechecked(&checks).refuting(
                    "infeasible",
                    "false_infeasibility",
                    "the returned candidate is feasible",
                ),
            ),
            Status::Infeasible => {
                // A feasible start contradicts the claim too. A start that
                // cannot be evaluated (an overflow) proves nothing.
                let start = self.payload.starts[task].as_ref().map(|s| {
                    validations += 1;
                    (s, check::evaluate(&self.model, s, tolerances).ok())
                });
                match start {
                    Some((s, Some(e))) if e.feasible_candidate => {
                        // Report the start, with its own evaluation.
                        x = Some(s.clone());
                        shown = Some(e);
                        shown_residuals = None;
                        (
                            "feasible",
                            Evidence::rechecked(&checks).refuting(
                                "infeasible",
                                "false_infeasibility",
                                format!("start {task} is feasible"),
                            ),
                        )
                    }
                    _ => ("infeasible", Evidence::claimed(recomputed)),
                }
            }
            Status::Unbounded | Status::InfeasibleOrUnbounded => {
                (r.status.as_str(), Evidence::claimed(recomputed))
            }
            _ if feasible == Some(true) => ("feasible", Evidence::rechecked(&checks)),
            _ => ("none", Evidence::claimed(recomputed)),
        };
        let point_feasible = shown.as_ref().map(|e| e.feasible_candidate);
        let objective = shown.as_ref().map(|e| e.objective);
        let primal = objective.filter(|_| point_feasible == Some(true));
        // The solver's bound is published only in order with the point the
        // outcome shows: a bound beyond a feasible point's objective
        // contradicts it, so it stays in details as reported_best_bound.
        let reported_bound = r.best_bound.filter(|b| b.is_finite());
        let (dual, rejected_bound) = match (primal, reported_bound) {
            (Some(p), Some(b))
                if !ObjectiveBounds {
                    sense: self.model.sense().as_str().into(),
                    primal: Some(p),
                    dual: Some(b),
                }
                .ordered() =>
            {
                (None, Some(b))
            }
            (_, b) => (b, None),
        };
        // The gap of the shown point against that bound, never of the
        // solver's own evaluation; an overflow leaves it out.
        let gap = match (primal, dual) {
            (Some(p), Some(b)) => Gap::try_new(self.model.sense(), p, b)
                .ok()
                .map(|g| json!({"absolute":scalar(g.absolute),"relative":scalar(g.relative)})),
            _ => None,
        };
        let lp_residuals = shown_residuals.map(|k| json!({"primal":scalar(k.primal),"stationarity":scalar(k.stationarity),"complementarity":scalar(k.complementarity),"dual_sign":scalar(k.dual_sign),"duality_gap":scalar(k.duality_gap)}));
        let mut details = json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),"best_bound":dual.map(scalar),"gap":gap,"primal_violation":shown.as_ref().map(|e|scalar(e.primal_violation)),"integrality_violation":shown.as_ref().map(|e|scalar(e.integrality_violation)),"lp_residuals":lp_residuals,"raw_status":r.backend.raw_status,"warm_start":r.backend.start_status.as_str(),"engine_version":r.backend.version,"effective_options":r.backend.effective_options,"message":r.message,"exact_certificate":false});
        if let Some(note) = note {
            details["note"] = json!(note);
        }
        if let Some(b) = rejected_bound {
            details["reported_best_bound"] = scalar(b);
            details["bound_note"] = json!(format!(
                "the solver's bound {b} contradicts the objective {} of the returned feasible point; it is not reported as objective_bounds.dual",
                primal.expect("a rejected bound has a primal")
            ));
        }
        // Only when the outcome shows the solver's own candidate.
        if let Some(reported) = mismatch.filter(|_| x == r.candidate) {
            details["reported_objective"] = scalar(reported);
        }
        Ok(TaskResult {
            outcome: Outcome {
                termination: r.status.as_str().into(),
                claim: claim.into(),
                feasible: point_feasible,
                objective,
                x,
                details,
                objective_bounds: objective_bounds(&self.model, primal, dual),
                evidence: Some(evidence),
                ..Outcome::default()
            },
            work: work(r.nodes, r.iterations, validations),
        })
    }
}
impl Prepared for Run {
    fn tasks(&self) -> usize {
        self.payload.starts.len()
    }
    fn cancel(&self) {
        self.cancel.cancel();
    }
    fn solve(&self, task: usize, remaining: f64) -> Result<TaskResult> {
        let settings = NativeOptions {
            time_limit_seconds: Some(remaining),
            ..self.settings.clone()
        };
        let warm = self.payload.starts[task]
            .clone()
            .map(WarmStart::from_primal);
        match NativeSolver::new(settings).solve(
            &self.model,
            &self.options,
            warm.as_ref(),
            &self.cancel,
        ) {
            Ok(r) => self.judge(task, r),
            // A well-formed model outside the native domain (for example a
            // MIP whose bounds stay infinite after propagation) is the
            // solver's answer for this model, not a worker failure. It is
            // refused before any node or simplex iteration.
            Err(Error::Unsupported(message)) => Ok(TaskResult {
                outcome: Outcome {
                    termination: "unsupported".into(),
                    claim: "none".into(),
                    details: json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),"message":message,"engine_version":ENGINE_VERSION,"exact_certificate":false}),
                    evidence: Some(Evidence::claimed(&[])),
                    ..Outcome::default()
                },
                work: work(0, 0, 0),
            }),
            Err(e) => Err(e.into()),
        }
    }
}
impl Prepared for Evaluate {
    fn tasks(&self) -> usize {
        self.points.len()
    }
    fn cancel(&self) {} // One evaluation per task; tasks stop between points.
    fn solve(&self, task: usize, _: f64) -> Result<TaskResult> {
        let x = &self.points[task];
        let e = match check::evaluate(&self.model, x, &self.tolerances) {
            Ok(e) => e,
            // A finite point whose objective or a row activity overflows
            // has no usable value; it is an answer for this point, not a
            // job failure.
            Err(error) => {
                return Ok(TaskResult {
                    outcome: Outcome {
                        termination: "evaluated".into(),
                        claim: "none".into(),
                        feasible: None,
                        x: Some(x.clone()),
                        details: json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),
                            "message":error.to_string()}),
                        evidence: Some(Evidence::claimed(&[])),
                        ..Outcome::default()
                    },
                    work: [("evaluations".into(), 1)].into(),
                });
            }
        };
        let feasible = e.feasible_candidate;
        Ok(TaskResult {
            outcome: Outcome {
                termination: "evaluated".into(),
                claim: if feasible { "feasible" } else { "none" }.into(),
                feasible: Some(feasible),
                objective: Some(e.objective),
                x: Some(x.clone()),
                details: json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),
                    "primal_violation":scalar(e.primal_violation),"integrality_violation":scalar(e.integrality_violation),
                    "tolerances":{"feasibility":self.tolerances.feasibility,"integrality":self.tolerances.integrality}}),
                objective_bounds: objective_bounds(
                    &self.model,
                    feasible.then_some(e.objective),
                    None,
                ),
                evidence: Some(Evidence::rechecked(&point_checks(&self.model))),
                ..Outcome::default()
            },
            work: [("evaluations".into(), 1)].into(),
        })
    }
}
fn main() {
    worker::run(Adapter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::Duals;
    use optimization_job_api::storage::digest;
    use std::{fs, path::PathBuf, sync::atomic::AtomicUsize};

    /// A fresh, empty directory.
    fn fresh_directory() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lp-ip-job-worker-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    /// A fresh directory holding `model` as `model.json`.
    fn directory(model: &Value) -> (PathBuf, String) {
        let dir = fresh_directory();
        let bytes = serde_json::to_vec(model).unwrap();
        fs::write(dir.join("model.json"), &bytes).unwrap();
        (dir, digest(&bytes))
    }
    /// The tutorial model: minimize 2 x0 + 3 x1 + 5 with x0 + x1 = 1.
    fn model(kind: &str) -> Value {
        json!({"schema_version":MODEL_SCHEMA,"sense":"minimize","objective":[2.0,3.0],"objective_offset":5.0,
               "variable_lower":[0.0,0.0],"variable_upper":[1.0,1.0],"kinds":[kind,kind],
               "matrix":{"indptr":[0,2],"indices":[0,1],"values":[1.0,1.0]},"row_lower":[1.0],"row_upper":[1.0]})
    }
    fn job(sha256: &str, threads: usize) -> JobSpec {
        serde_json::from_value(json!({"schema_version":"optimization.job/0.1","request_key":"test","worker":"lp-ip",
            "payload_schema":PROFILE,"payload":{"model_input":"model","starts":[null,null],"node_limit":1000,
            "iteration_limit":10000,"feasibility_tolerance":1e-7,"integrality_tolerance":1e-7,"presolve":true},
            "resources":{"cpu_slots":8,"outer_workers":1,"inner_threads":threads},
            "limits":{"wall_time_seconds":30.0,"grace_seconds":1.0},
            "inputs":[{"name":"model","file":"model.json","sha256":sha256}]}))
        .unwrap()
    }
    fn prepare(model: &Value, edit: impl FnOnce(&mut JobSpec)) -> Result<Box<dyn Prepared>> {
        let (dir, sha256) = directory(model);
        let mut s = job(&sha256, 1);
        edit(&mut s);
        let prepared = Adapter.prepare(&mut s, &dir);
        fs::remove_dir_all(dir).unwrap();
        prepared
    }
    fn solve(model: &Value, edit: impl FnOnce(&mut JobSpec)) -> TaskResult {
        prepare(model, edit).unwrap().solve(0, 30.0).unwrap()
    }
    fn option<'a>(o: &'a Outcome, key: &str) -> &'a str {
        o.details["effective_options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p[0] == key)
            .and_then(|p| p[1].as_str())
            .unwrap()
    }

    #[test]
    fn mip_and_lp_reach_the_known_optimum_with_checked_claims() {
        for (kind, threads) in [("binary", 1), ("continuous", 1), ("continuous", 8)] {
            let r = solve(&model(kind), |s| s.resources.inner_threads = threads);
            let o = &r.outcome;
            assert_eq!(
                (o.termination.as_str(), o.claim.as_str()),
                ("optimal", "numerical_optimum")
            );
            assert_eq!(o.feasible, Some(true));
            assert!((o.objective.unwrap() - 7.0).abs() < 1e-9);
            assert_eq!(o.x.as_deref(), Some(&[1.0, 0.0][..]));
            // The conservative bound may lie a few ulps below the objective.
            let bound = o.details["best_bound"].as_f64().unwrap();
            assert!(bound <= 7.0 && 7.0 - bound < 1e-9);
            assert_eq!(o.details["lp_residuals"].is_null(), kind == "binary");
            assert_eq!(r.work["validation_evaluations"], 1);
            // Re-evaluated point; an LP's duals also pass the KKT acceptance.
            let e = o.evidence.as_ref().unwrap();
            assert_eq!(e.level, "rechecked");
            let expected: &[&str] = if kind == "binary" {
                &["objective", "bounds", "constraints", "integrality"]
            } else {
                &["objective", "bounds", "constraints", "kkt"]
            };
            assert_eq!(e.checks, expected);
            let b = o.objective_bounds.as_ref().unwrap();
            assert_eq!((b.sense.as_str(), b.primal), ("minimize", o.objective));
            assert_eq!(b.dual, o.details["best_bound"].as_f64());
            o.check().unwrap();
            assert_eq!(option(o, "presolve"), "true");
            assert_eq!(option(o, "mip_presolve"), "true");
        }
    }
    #[test]
    fn infeasible_and_unbounded_models_carry_their_claims() {
        let mut infeasible = model("continuous");
        infeasible["row_lower"] = json!([3.0]);
        infeasible["row_upper"] = json!([3.0]);
        let o = solve(&infeasible, |_| {}).outcome;
        assert_eq!(
            (o.termination.as_str(), o.claim.as_str()),
            ("infeasible", "infeasible")
        );
        assert!(o.x.is_none() && o.objective.is_none() && o.objective_bounds.is_none());
        assert_eq!(o.evidence.as_ref().unwrap().level, "claimed");
        o.check().unwrap();
        let unbounded = json!({"schema_version":MODEL_SCHEMA,"sense":"maximize","objective":[1.0],"objective_offset":0.0,
            "variable_lower":[0.0],"variable_upper":[null],"kinds":["continuous"],
            "matrix":{"indptr":[0],"indices":[],"values":[]},"row_lower":[],"row_upper":[]});
        let o = solve(&unbounded, |_| {}).outcome;
        assert_eq!(
            (o.termination.as_str(), o.claim.as_str()),
            ("unbounded", "unbounded")
        );
        assert_eq!(o.evidence.as_ref().unwrap().level, "claimed");
        o.check().unwrap();
    }

    /// A worker run of `model` without solving, for judging synthetic results.
    fn run_for(model: &Value, starts: Value) -> Run {
        let payload: Payload =
            serde_json::from_value(json!({"model_input":"model","starts":starts,
            "node_limit":1000,"iteration_limit":10000,"feasibility_tolerance":1e-7,
            "integrality_tolerance":1e-7,"presolve":true}))
            .unwrap();
        Run {
            model: serde_json::from_value::<ModelFile>(model.clone())
                .unwrap()
                .model()
                .unwrap(),
            settings: payload.settings(),
            payload,
            options: Options {
                threads: 1,
                tolerances: Tolerances {
                    feasibility: 1e-7,
                    integrality: 1e-7,
                },
            },
            cancel: CancellationToken::default(),
        }
    }
    fn native(status: Status, candidate: Option<Vec<f64>>, duals: Option<Duals>) -> SolveResult {
        SolveResult {
            status,
            candidate,
            evaluation: None,
            duals,
            best_bound: None,
            nodes: 0,
            iterations: 0,
            message: String::new(),
            backend: Default::default(),
        }
    }
    fn refuted(o: &Outcome) -> (&str, &str, &str) {
        let r = o.evidence.as_ref().unwrap().refuted.as_ref().unwrap();
        (o.claim.as_str(), r.claim.as_str(), r.verdict.as_str())
    }

    #[test]
    fn native_claims_that_fail_their_checks_are_refuted() {
        let lp = run_for(&model("continuous"), json!([null]));
        // Optimal, but the point violates x0 + x1 = 1.
        let o = lp
            .judge(0, native(Status::Optimal, Some(vec![0.0, 0.0]), None))
            .unwrap()
            .outcome;
        assert_eq!(
            refuted(&o),
            ("none", "numerical_optimum", "infeasible_point")
        );
        assert_eq!(o.feasible, Some(false));
        assert!(o.objective_bounds.is_none());
        o.check().unwrap();
        // Optimal and feasible, but zero duals leave stationarity |c| = 3.
        let zero = Duals {
            row_duals: vec![0.0],
            reduced_costs: vec![0.0, 0.0],
        };
        let o = lp
            .judge(0, native(Status::Optimal, Some(vec![1.0, 0.0]), Some(zero)))
            .unwrap()
            .outcome;
        assert_eq!(
            refuted(&o),
            ("feasible", "numerical_optimum", "false_optimality")
        );
        assert!(
            o.evidence
                .as_ref()
                .unwrap()
                .checks
                .contains(&"kkt".to_string())
        );
        o.check().unwrap();
        // The correct duals pass: y = 2, reduced costs (0, 1).
        let right = Duals {
            row_duals: vec![2.0],
            reduced_costs: vec![0.0, 1.0],
        };
        let o = lp
            .judge(
                0,
                native(Status::Optimal, Some(vec![1.0, 0.0]), Some(right)),
            )
            .unwrap()
            .outcome;
        assert_eq!(o.claim, "numerical_optimum");
        // Optimal without a candidate: nothing was checked, nothing refuted.
        let o = lp
            .judge(0, native(Status::Optimal, None, None))
            .unwrap()
            .outcome;
        assert_eq!(o.claim, "none");
        let e = o.evidence.as_ref().unwrap();
        assert!(e.level == "claimed" && e.refuted.is_none());
        assert!(o.details["note"].is_string());
        o.check().unwrap();
        // Residuals that overflow cannot confirm an optimum.
        let huge = Duals {
            row_duals: vec![f64::MAX],
            reduced_costs: vec![-f64::MAX, 0.0],
        };
        let o = lp
            .judge(0, native(Status::Optimal, Some(vec![1.0, 0.0]), Some(huge)))
            .unwrap()
            .outcome;
        assert_eq!(
            refuted(&o),
            ("feasible", "numerical_optimum", "false_optimality")
        );
        // An infeasibility claim with a feasible returned point is refuted.
        let o = lp
            .judge(0, native(Status::Infeasible, Some(vec![0.0, 1.0]), None))
            .unwrap()
            .outcome;
        assert_eq!(
            refuted(&o),
            ("feasible", "infeasible", "false_infeasibility")
        );
        o.check().unwrap();
        // An infeasibility claim with a feasible start is refuted by the start;
        // the details describe the start, not the native candidate.
        let mip = run_for(&model("binary"), json!([[0.0, 1.0], null]));
        let r = mip
            .judge(0, native(Status::Infeasible, Some(vec![0.0, 0.0]), None))
            .unwrap();
        assert_eq!(
            refuted(&r.outcome),
            ("feasible", "infeasible", "false_infeasibility")
        );
        assert_eq!(r.outcome.x.as_deref(), Some(&[0.0, 1.0][..]));
        assert_eq!(r.outcome.objective, Some(8.0));
        assert_eq!(r.outcome.details["primal_violation"], 0.0);
        assert_eq!(r.work["validation_evaluations"], 2);
        r.outcome.check().unwrap();
        // Unbounded keeps the solver's claim and reports a returned point.
        let o = lp
            .judge(0, native(Status::Unbounded, Some(vec![1.0, 0.0]), None))
            .unwrap()
            .outcome;
        assert_eq!((o.claim.as_str(), o.feasible), ("unbounded", Some(true)));
        let e = o.evidence.as_ref().unwrap();
        assert_eq!((e.level.as_str(), e.checks.len()), ("claimed", 3));
        o.check().unwrap();
        // Without a start, the claim stands as the solver's.
        let o = mip
            .judge(1, native(Status::Infeasible, None, None))
            .unwrap()
            .outcome;
        assert_eq!(o.claim, "infeasible");
        assert_eq!(o.evidence.unwrap().level, "claimed");
        // A limit with a feasible incumbent is only a feasible point.
        let o = mip
            .judge(1, native(Status::NodeLimit, Some(vec![1.0, 0.0]), None))
            .unwrap()
            .outcome;
        assert_eq!(
            (o.termination.as_str(), o.claim.as_str()),
            ("node_limit", "feasible")
        );
        assert_eq!(o.evidence.as_ref().unwrap().level, "rechecked");
    }

    /// `native` with the solver's own evaluation of the candidate, its
    /// objective replaced by `own`, and a bound.
    fn reported(
        run: &Run,
        status: Status,
        x: Vec<f64>,
        own: f64,
        bound: Option<f64>,
        duals: Option<Duals>,
    ) -> SolveResult {
        let mut evaluation = check::evaluate(&run.model, &x, &run.options.tolerances).unwrap();
        evaluation.objective = own;
        SolveResult {
            evaluation: Some(evaluation),
            best_bound: bound,
            ..native(status, Some(x), duals)
        }
    }

    #[test]
    fn bounds_and_gaps_describe_the_point_the_outcome_shows() {
        // Review LP-01: optimal status, a stale own objective (8) and bound
        // (8) for the point [1, 0], whose objective is 7, and duals that fail
        // the KKT check. The claim was refuted, but the dual bound 8 and a
        // zero gap were still published.
        let lp = run_for(&model("continuous"), json!([null]));
        let zero = Duals {
            row_duals: vec![0.0],
            reduced_costs: vec![0.0, 0.0],
        };
        let o = lp
            .judge(
                0,
                reported(
                    &lp,
                    Status::Optimal,
                    vec![1.0, 0.0],
                    8.0,
                    Some(8.0),
                    Some(zero),
                ),
            )
            .unwrap()
            .outcome;
        o.check().unwrap();
        assert_eq!(
            refuted(&o),
            ("feasible", "numerical_optimum", "false_optimality")
        );
        assert_eq!(o.objective, Some(7.0));
        let b = o.objective_bounds.as_ref().unwrap();
        assert_eq!((b.primal, b.dual), (Some(7.0), None));
        assert!(o.details["best_bound"].is_null() && o.details["gap"].is_null());
        assert_eq!(o.details["reported_best_bound"], 8.0);
        assert!(
            o.details["bound_note"]
                .as_str()
                .unwrap()
                .contains("contradicts")
        );
        assert_eq!(o.details["reported_objective"], 8.0);
        // A consistent optimum keeps its bound and a zero gap.
        let right = Duals {
            row_duals: vec![2.0],
            reduced_costs: vec![0.0, 1.0],
        };
        let o = lp
            .judge(
                0,
                reported(
                    &lp,
                    Status::Optimal,
                    vec![1.0, 0.0],
                    7.0,
                    Some(7.0),
                    Some(right),
                ),
            )
            .unwrap()
            .outcome;
        o.check().unwrap();
        assert_eq!(o.claim, "numerical_optimum");
        let b = o.objective_bounds.as_ref().unwrap();
        assert_eq!((b.primal, b.dual), (Some(7.0), Some(7.0)));
        assert_eq!(o.details["gap"], json!({"absolute":0.0,"relative":0.0}));
        assert!(o.details.get("reported_best_bound").is_none());
        assert!(o.details.get("reported_objective").is_none());
        // A MIP optimum (no duals) whose own objective its point does not
        // evaluate to is refuted; its bound, in order with the point, stays.
        let mip = run_for(&model("binary"), json!([[0.0, 1.0], null]));
        let o = mip
            .judge(
                1,
                reported(&mip, Status::Optimal, vec![1.0, 0.0], 8.0, Some(7.0), None),
            )
            .unwrap()
            .outcome;
        o.check().unwrap();
        assert_eq!(
            refuted(&o),
            ("feasible", "numerical_optimum", "objective_mismatch")
        );
        assert_eq!(o.details["reported_objective"], 8.0);
        assert_eq!(o.objective_bounds.as_ref().unwrap().dual, Some(7.0));
        // A valid bound below an incumbent gives its gap.
        let o = mip
            .judge(
                1,
                reported(
                    &mip,
                    Status::NodeLimit,
                    vec![0.0, 1.0],
                    8.0,
                    Some(7.5),
                    None,
                ),
            )
            .unwrap()
            .outcome;
        o.check().unwrap();
        assert_eq!(o.details["gap"], json!({"absolute":0.5,"relative":0.0625}));
        // A feasible start that refutes infeasibility is shown with its own
        // objective (8); a bound beyond it (9) is not published.
        let o = mip
            .judge(
                0,
                reported(
                    &mip,
                    Status::Infeasible,
                    vec![0.0, 0.0],
                    5.0,
                    Some(9.0),
                    None,
                ),
            )
            .unwrap()
            .outcome;
        o.check().unwrap();
        assert_eq!(
            refuted(&o),
            ("feasible", "infeasible", "false_infeasibility")
        );
        assert_eq!(
            (o.objective, o.details["reported_best_bound"].as_f64()),
            (Some(8.0), Some(9.0))
        );
        assert_eq!(o.objective_bounds.as_ref().unwrap().dual, None);
        // The solver's objective of its own candidate is not attached to the
        // start the outcome shows.
        assert!(o.details.get("reported_objective").is_none());
        // Maximization reverses the order: an upper bound below the point's
        // objective is a contradiction, one above it is a valid bound.
        let mut maximize = model("binary");
        maximize["sense"] = json!("maximize");
        let max = run_for(&maximize, json!([null]));
        for (bound, kept) in [(6.5, false), (8.0, true)] {
            let o = max
                .judge(
                    0,
                    reported(
                        &max,
                        Status::NodeLimit,
                        vec![1.0, 0.0],
                        7.0,
                        Some(bound),
                        None,
                    ),
                )
                .unwrap()
                .outcome;
            o.check().unwrap();
            assert_eq!(
                o.objective_bounds.as_ref().unwrap().dual.is_some(),
                kept,
                "{bound}"
            );
            assert_eq!(o.details["gap"].is_object(), kept, "{bound}");
        }
        // A gap that overflows is left out instead of failing the task.
        let mut huge = model("continuous");
        huge["objective"] = json!([1e308, 1e308]);
        huge["objective_offset"] = json!(0.0);
        let big = run_for(&huge, json!([null]));
        let r = big
            .judge(
                0,
                reported(
                    &big,
                    Status::NodeLimit,
                    vec![1.0, 0.0],
                    1e308,
                    Some(-1e308),
                    None,
                ),
            )
            .unwrap();
        r.outcome.check().unwrap();
        assert_eq!(
            r.outcome.objective_bounds.as_ref().unwrap().dual,
            Some(-1e308)
        );
        assert!(r.outcome.details["gap"].is_null());
    }

    #[test]
    fn evaluate_checks_given_points_without_solving() {
        let (dir, sha256) = directory(&model("binary"));
        let mut s = job(&sha256, 1);
        s.payload_schema = EVALUATE_PROFILE.into();
        s.payload = json!({"model_input":"model","points":[[1.0,0.0],[0.0,0.0],[0.5,0.5],[1e308,1e308]],
            "feasibility_tolerance":1e-7,"integrality_tolerance":1e-7});
        assert_top_level_conforms(
            &Adapter.describe()["payload_schemas"][EVALUATE_PROFILE],
            &s.payload,
        );
        let p = Adapter.prepare(&mut s, &dir).unwrap();
        assert_eq!(p.tasks(), 4);
        let outcomes: Vec<Outcome> = (0..4).map(|t| p.solve(t, 1.0).unwrap().outcome).collect();
        assert_eq!(
            outcomes
                .iter()
                .map(|o| o.claim.as_str())
                .collect::<Vec<_>>(),
            ["feasible", "none", "none", "none"]
        );
        // The row activity overflows: no value, an answer for this point only.
        assert!(outcomes[3].objective.is_none() && outcomes[3].feasible.is_none());
        assert!(
            outcomes[3].details["message"]
                .as_str()
                .unwrap()
                .contains("overflow")
        );
        assert_eq!(outcomes[0].objective, Some(7.0));
        assert_eq!(
            outcomes[0].objective_bounds.as_ref().unwrap().primal,
            Some(7.0)
        );
        assert_eq!(outcomes[1].details["primal_violation"], 1.0);
        assert_eq!(outcomes[2].details["integrality_violation"], 0.5);
        for o in &outcomes {
            assert_eq!(o.termination, "evaluated");
            o.check().unwrap();
        }
        for (edit, message) in [
            (json!({"points":[]}), "1..1000000"),
            (json!({"points":[[1.0]]}), "point 0"),
            (json!({"feasibility_tolerance":0.0}), "tolerance"),
        ] {
            let mut bad = s.clone();
            for (k, v) in edit.as_object().unwrap() {
                bad.payload[k] = v.clone();
            }
            let err = Adapter.prepare(&mut bad, &dir).err().unwrap().to_string();
            assert!(err.contains(message), "{message}: {err}");
        }
        s.resources.inner_threads = 2;
        assert!(Adapter.prepare(&mut s, &dir).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_model_outside_the_native_domain_is_an_outcome_not_a_failure() {
        // Native MIP needs finite bounds; the free continuous column stays free.
        let mut free = model("integer");
        free["kinds"] = json!(["integer", "continuous"]);
        free["variable_lower"] = json!([0.0, null]);
        free["variable_upper"] = json!([1.0, null]);
        free["row_lower"] = json!([1.0]);
        free["row_upper"] = json!([null]);
        let r = solve(&free, |_| {});
        assert_eq!(
            (r.outcome.termination.as_str(), r.outcome.claim.as_str()),
            ("unsupported", "none")
        );
        assert!(
            r.outcome.details["message"]
                .as_str()
                .unwrap()
                .contains("finite")
        );
        assert!(r.outcome.x.is_none() && r.outcome.feasible.is_none());
        assert_eq!(r.work, work(0, 0, 0));
    }
    #[test]
    fn presolve_off_turns_off_every_reduction_and_reports_it() {
        let r = solve(&model("binary"), |s| s.payload["presolve"] = json!(false));
        let o = &r.outcome;
        assert_eq!(o.claim, "numerical_optimum");
        for key in [
            "presolve",
            "singleton_presolve",
            "root_propagation",
            "mip_presolve",
            "coefficient_tightening",
        ] {
            assert_eq!(option(o, key), "false", "{key}");
        }
        // Only the switches themselves change; the rest keeps native defaults.
        let on = Payload {
            presolve: true,
            ..serde_json::from_value(job("0", 1).payload).unwrap()
        }
        .settings();
        let defaults = NativeOptions::default();
        assert_eq!(
            (on.mip_presolve, on.coefficient_tightening, on.dual_presolve),
            (
                defaults.mip_presolve,
                defaults.coefficient_tightening,
                defaults.dual_presolve
            )
        );
        assert_eq!(
            (on.component_threads, on.packing_workers, on.tree_threads),
            (1, 1, 1)
        );
    }
    #[test]
    fn mip_starts_are_candidates_and_lp_starts_are_rejected() {
        let r = solve(&model("binary"), |s| {
            s.payload["starts"] = json!([[0.0, 1.0]])
        });
        // The feasible start is evaluated as a candidate, not rejected.
        assert_eq!(r.outcome.details["warm_start"], "evaluated");
        assert!((r.outcome.objective.unwrap() - 7.0).abs() < 1e-9);
        assert!(
            prepare(&model("continuous"), |s| s.payload["starts"] =
                json!([[1.0, 0.0]]))
            .is_err()
        );
        assert!(prepare(&model("binary"), |s| s.payload["starts"] = json!([[1.0]])).is_err());
    }
    #[test]
    fn invalid_requests_are_rejected_before_any_solve() {
        type Edit = Box<dyn FnOnce(&mut JobSpec)>;
        let edits: Vec<Edit> = vec![
            Box::new(|s| s.payload_schema = "lp-ip.solve/2".into()),
            Box::new(|s| s.resources.inner_threads = 9),
            Box::new(|s| s.payload["unknown"] = json!(1)),
            Box::new(|s| s.payload["starts"] = json!([])),
            Box::new(|s| s.payload["node_limit"] = json!(0)),
            Box::new(|s| s.payload["iteration_limit"] = json!(0)),
            Box::new(|s| s.payload["feasibility_tolerance"] = json!(0.0)),
            Box::new(|s| s.payload["model_input"] = json!("other")),
            Box::new(|s| s.inputs[0].sha256 = "0".repeat(64)),
        ];
        for (i, edit) in edits.into_iter().enumerate() {
            assert!(prepare(&model("binary"), edit).is_err(), "edit {i}");
        }
        let mut bad = model("binary");
        bad["schema_version"] = json!("lp-ip.model/2");
        assert!(prepare(&bad, |_| {}).is_err());
        bad = model("binary");
        bad["matrix"]["indptr"] = json!([1]);
        assert!(prepare(&bad, |_| {}).is_err());
    }
    #[test]
    fn description_reports_the_linked_engine_and_profile() {
        let d = Adapter.describe();
        assert_eq!(d["build"]["engine_version"], ENGINE_VERSION);
        assert_eq!(d["profiles"], json!([PROFILE, EVALUATE_PROFILE]));
        assert_eq!(d["protocol"], "optimization.worker/0.2");
        assert_eq!(d["resources"]["inner_threads_max"], INNER_THREADS_MAX);
        assert_eq!(scalar(f64::INFINITY), "+infinity");
        assert_eq!(scalar(f64::NEG_INFINITY), "-infinity");
        assert_eq!(scalar(2.5), 2.5);
    }

    /// The top-level `required` and `additionalProperties: false` rules of
    /// `schema`. The Python tests check the complete schemas with jsonschema.
    fn assert_top_level_conforms(schema: &Value, value: &Value) {
        assert_eq!(schema["additionalProperties"], false);
        for key in schema["required"].as_array().unwrap() {
            let key = key.as_str().unwrap();
            assert!(value.get(key).is_some(), "missing {key} in {value}");
        }
        let properties = schema["properties"].as_object().unwrap();
        for key in value.as_object().unwrap().keys() {
            assert!(properties.contains_key(key), "unknown {key} in {value}");
        }
    }
    /// The job a client builds from a `describe` example: the envelope
    /// fields, and each input written to `dir` with the SHA-256 of the bytes
    /// written.
    fn job_from_example(example: &Value, dir: &Path, request_key: &str) -> JobSpec {
        let inputs: Vec<Value> = example["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| {
                let bytes = serde_json::to_vec_pretty(&input["content"]).unwrap();
                let file = input["file"].as_str().unwrap();
                fs::write(dir.join(file), &bytes).unwrap();
                json!({"name":input["name"],"file":file,"sha256":digest(&bytes)})
            })
            .collect();
        serde_json::from_value(
            json!({"schema_version":"optimization.job/0.1","request_key":request_key,
            "worker":"lp-ip","payload_schema":example["profile"],"payload":example["payload"],
            "resources":example["resources"],"limits":example["limits"],"inputs":inputs}),
        )
        .unwrap()
    }

    #[test]
    fn every_profile_has_a_payload_schema_and_an_example() {
        let d = Adapter.describe();
        let draft = "https://json-schema.org/draft/2020-12/schema";
        let profiles = d["profiles"].as_array().unwrap();
        let examples = d["examples"].as_array().unwrap();
        assert_eq!(
            d["payload_schemas"].as_object().unwrap().len(),
            profiles.len()
        );
        for profile in profiles {
            let p = profile.as_str().unwrap();
            let schema = &d["payload_schemas"][p];
            assert_eq!(schema["$schema"], draft, "{p}");
            assert_eq!(schema["$id"], format!("urn:optimization-jobs:payload:{p}"));
            assert!(
                examples.iter().any(|e| e["profile"] == *profile),
                "{p} has no example"
            );
        }
        let inputs = d["input_schemas"].as_object().unwrap();
        assert!(inputs.contains_key(MODEL_SCHEMA));
        for (format, schema) in inputs {
            assert_eq!(schema["$schema"], draft, "{format}");
            assert_eq!(
                schema["$id"],
                format!("urn:optimization-jobs:input:{format}")
            );
        }
        for e in examples {
            assert!(profiles.contains(&e["profile"]), "{e}");
            assert!(!e["description"].as_str().unwrap().is_empty());
            assert_eq!(
                e["resources"],
                json!({"cpu_slots":1,"outer_workers":1,"inner_threads":1})
            );
            assert_top_level_conforms(
                &d["payload_schemas"][e["profile"].as_str().unwrap()],
                &e["payload"],
            );
            for input in e["inputs"].as_array().unwrap() {
                let format = input["format"].as_str().unwrap();
                assert!(inputs.contains_key(format), "unknown format {format}");
                assert_top_level_conforms(&inputs[format], &input["content"]);
            }
        }
        // The payloads and models these tests build follow the same schemas.
        assert_top_level_conforms(&d["payload_schemas"][PROFILE], &job("0", 1).payload);
        assert_top_level_conforms(&inputs[MODEL_SCHEMA], &model("binary"));
        // Compact: the convention asks for well under 256 KiB.
        assert!(serde_json::to_vec(&d).unwrap().len() < 64 * 1024);
    }

    #[test]
    fn every_describe_example_is_a_valid_job_with_the_described_outcome() {
        let examples = Adapter.describe()["examples"].as_array().unwrap().clone();
        // Per example: each task's claim and objective, as its description says.
        let expected: [&[(&str, Option<f64>)]; 3] = [
            &[("numerical_optimum", Some(9.0))],
            &[("numerical_optimum", Some(18.0))],
            &[
                ("feasible", Some(18.0)),
                ("feasible", Some(16.0)),
                ("none", Some(23.0)),
            ],
        ];
        assert_eq!(examples.len(), expected.len());
        for (i, (example, expected)) in examples.iter().zip(expected).enumerate() {
            let dir = fresh_directory();
            let mut s = job_from_example(example, &dir, &format!("example-{i}"));
            // What the runner and the worker SDK check around `prepare`.
            s.validate().unwrap();
            let p = Adapter
                .prepare(&mut s, &dir)
                .unwrap_or_else(|e| panic!("example {i}: {e}"));
            s.task_count = Some(p.tasks());
            s.validate().unwrap();
            assert_eq!(p.tasks(), expected.len(), "example {i}");
            for (t, (claim, objective)) in expected.iter().enumerate() {
                let o = p.solve(t, 30.0).unwrap().outcome;
                o.check().unwrap();
                assert_eq!(o.claim, *claim, "example {i}, task {t}");
                let close = |a: f64, b: f64| (a - b).abs() < 1e-7;
                assert!(
                    o.objective
                        .zip(*objective)
                        .is_some_and(|(a, b)| close(a, b)),
                    "example {i}, task {t}: {:?}",
                    o.objective
                );
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
