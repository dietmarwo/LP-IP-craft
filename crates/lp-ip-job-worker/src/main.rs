//! LP/MIP artifact adapter for the common optimization job protocol.
#![forbid(unsafe_code)]
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, Error, LpIpSolver, Model, ModelData, Options, Sense,
    SolveResult, Status, Tolerances, VariableKind, WarmStart, check,
};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
use optimization_job_api::{
    JobSpec, Result,
    result::{Evidence, ObjectiveBounds, Outcome},
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
            "points":"1..1000000 complete points of the model's dimension; one task per point",
            "result":"claim feasible within the tolerances, else none; objective, violations and objective_bounds.primal for a feasible point"});
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
        let residuals = match (&r.candidate, &r.duals) {
            (Some(x), Some(d)) => Some(check::lp_residuals(&self.model, x, d)?),
            _ => None,
        };
        let kkt = residuals
            .as_ref()
            .map(|k| kkt_accepted(&self.model, k, tolerances, self.settings.dual_tolerance));
        let mut x = r.candidate.clone();
        let mut objective = checked.as_ref().map(|e| e.objective);
        let mut point_feasible = feasible;
        let violation = |e: &lp_ip_craft::Evaluation| {
            format!(
                "primal violation {} (tolerance {}), integrality violation {} (tolerance {})",
                e.primal_violation,
                tolerances.feasibility,
                e.integrality_violation,
                tolerances.integrality
            )
        };
        let (claim, evidence) = match r.status {
            Status::Optimal if feasible != Some(true) => {
                let message = checked
                    .as_ref()
                    .map_or("no candidate returned".into(), violation);
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
                let k = residuals.as_ref().expect("KKT judged from residuals");
                let message = format!(
                    "primal {}, stationarity {}, dual_sign {}, duality_gap {} fail the acceptance (dual tolerance {})",
                    k.primal,
                    k.stationarity,
                    k.dual_sign,
                    k.duality_gap,
                    self.settings.dual_tolerance
                );
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
            Status::Optimal => {
                let mut all = checks.clone();
                if kkt.is_some() {
                    all.push("kkt");
                }
                ("numerical_optimum", Evidence::rechecked(&all))
            }
            Status::Infeasible => {
                // A feasible returned point or start contradicts the claim.
                let start = self.payload.starts[task]
                    .as_ref()
                    .filter(|_| feasible != Some(true))
                    .map(|s| check::evaluate(&self.model, s, tolerances).map(|e| (s, e)))
                    .transpose()?;
                validations += u64::from(start.is_some());
                if feasible == Some(true) {
                    (
                        "feasible",
                        Evidence::rechecked(&checks).refuting(
                            "infeasible",
                            "false_infeasibility",
                            "the returned candidate is feasible",
                        ),
                    )
                } else if let Some((s, e)) = start.filter(|(_, e)| e.feasible_candidate) {
                    x = Some(s.clone());
                    objective = Some(e.objective);
                    point_feasible = Some(true);
                    (
                        "feasible",
                        Evidence::rechecked(&checks).refuting(
                            "infeasible",
                            "false_infeasibility",
                            format!("start {task} is feasible"),
                        ),
                    )
                } else {
                    let recomputed: &[&str] = if checked.is_some() { &checks } else { &[] };
                    ("infeasible", Evidence::claimed(recomputed))
                }
            }
            Status::Unbounded | Status::InfeasibleOrUnbounded => {
                let recomputed: &[&str] = if checked.is_some() { &checks } else { &[] };
                (r.status.as_str(), Evidence::claimed(recomputed))
            }
            _ if feasible == Some(true) => ("feasible", Evidence::rechecked(&checks)),
            _ => {
                let recomputed: &[&str] = if checked.is_some() { &checks } else { &[] };
                ("none", Evidence::claimed(recomputed))
            }
        };
        let primal = objective.filter(|_| point_feasible == Some(true));
        let gap = r
            .gap(self.model.sense())?
            .map(|g| json!({"absolute":scalar(g.absolute),"relative":scalar(g.relative)}));
        let lp_residuals = residuals.map(|k| json!({"primal":scalar(k.primal),"stationarity":scalar(k.stationarity),"complementarity":scalar(k.complementarity),"dual_sign":scalar(k.dual_sign),"duality_gap":scalar(k.duality_gap)}));
        Ok(TaskResult {
            outcome: Outcome {
                termination: r.status.as_str().into(),
                claim: claim.into(),
                feasible: point_feasible,
                objective,
                x,
                details: json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),"best_bound":r.best_bound.map(scalar),"gap":gap,"primal_violation":checked.as_ref().map(|e|scalar(e.primal_violation)),"integrality_violation":checked.as_ref().map(|e|scalar(e.integrality_violation)),"lp_residuals":lp_residuals,"raw_status":r.backend.raw_status,"warm_start":r.backend.start_status.as_str(),"engine_version":r.backend.version,"effective_options":r.backend.effective_options,"message":r.message,"exact_certificate":false}),
                objective_bounds: objective_bounds(&self.model, primal, r.best_bound),
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
        let e = check::evaluate(&self.model, x, &self.tolerances)?;
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

    /// A fresh directory holding `model` as `model.json`.
    fn directory(model: &Value) -> (PathBuf, String) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lp-ip-job-worker-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
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
        // An infeasibility claim with a feasible start is refuted by the start.
        let mip = run_for(&model("binary"), json!([[0.0, 1.0], null]));
        let r = mip
            .judge(0, native(Status::Infeasible, None, None))
            .unwrap();
        assert_eq!(
            refuted(&r.outcome),
            ("feasible", "infeasible", "false_infeasibility")
        );
        assert_eq!(r.outcome.x.as_deref(), Some(&[0.0, 1.0][..]));
        assert_eq!(r.outcome.objective, Some(8.0));
        assert_eq!(r.work["validation_evaluations"], 1);
        r.outcome.check().unwrap();
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

    #[test]
    fn evaluate_checks_given_points_without_solving() {
        let (dir, sha256) = directory(&model("binary"));
        let mut s = job(&sha256, 1);
        s.payload_schema = EVALUATE_PROFILE.into();
        s.payload = json!({"model_input":"model","points":[[1.0,0.0],[0.0,0.0],[0.5,0.5]],
            "feasibility_tolerance":1e-7,"integrality_tolerance":1e-7});
        let p = Adapter.prepare(&mut s, &dir).unwrap();
        assert_eq!(p.tasks(), 3);
        let outcomes: Vec<Outcome> = (0..3).map(|t| p.solve(t, 1.0).unwrap().outcome).collect();
        assert_eq!(
            outcomes
                .iter()
                .map(|o| o.claim.as_str())
                .collect::<Vec<_>>(),
            ["feasible", "none", "none"]
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
}
