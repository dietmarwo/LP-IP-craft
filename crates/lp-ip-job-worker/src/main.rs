//! LP/MIP artifact adapter for the common optimization job protocol.
#![forbid(unsafe_code)]
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, LpIpSolver, Model, ModelData, Options, Sense, Status,
    Tolerances, VariableKind, WarmStart, check,
};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
use optimization_job_api::{
    JobSpec, Result,
    result::Outcome,
    worker::{self, Prepared, TaskResult, Worker},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

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
        if self.schema_version != "lp-ip.model/1" {
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
struct Adapter;
struct Run {
    model: Model,
    payload: Payload,
    options: Options,
    cancel: CancellationToken,
}
impl Worker for Adapter {
    fn describe(&self) -> Value {
        let mut d = worker::description(
            "lp-ip-craft",
            "0.28.0",
            json!(["lp-ip.solve/1"]),
            json!([{"id":"lp-ip.model/1","input":"SHA-256 checked sparse CSR JSON artifact","warm_start":"complete MIP primal candidates only"}]),
            8,
        );
        d["cancellation"] = json!("native cooperative token plus runner process deadline");
        d
    }
    fn prepare(&self, s: &mut JobSpec, base: &Path) -> Result<Box<dyn Prepared>> {
        if s.payload_schema != "lp-ip.solve/1" || s.resources.inner_threads > 8 {
            return Err("lp-ip.solve/1 supports inner_threads=1..8".into());
        }
        let p: Payload = serde_json::from_value(s.payload.clone())?;
        if s.inputs.len() != 1
            || s.inputs[0].name != p.model_input
            || p.starts.is_empty()
            || p.starts.len() > 1_000_000
            || p.node_limit == 0
            || p.iteration_limit == 0
        {
            return Err(
                "one model input, nonempty starts, positive node/iteration limits required".into(),
            );
        }
        let model =
            serde_json::from_slice::<ModelFile>(&s.input_bytes(base, &p.model_input)?)?.model()?;
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
        s.payload = serde_json::to_value(&p)?;
        Ok(Box::new(Run {
            model,
            payload: p,
            options,
            cancel: CancellationToken::default(),
        }))
    }
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
impl Prepared for Run {
    fn tasks(&self) -> usize {
        self.payload.starts.len()
    }
    fn cancel(&self) {
        self.cancel.cancel();
    }
    fn solve(&self, task: usize, remaining: f64) -> Result<TaskResult> {
        // Internal component/frontier/residual parallelism is explicitly serial.
        // Only Options::threads may consume this task's inner budget.
        let settings = NativeOptions {
            time_limit_seconds: Some(remaining),
            node_limit: Some(self.payload.node_limit),
            iteration_limit: Some(self.payload.iteration_limit),
            presolve: self.payload.presolve,
            component_threads: 1,
            packing_workers: 1,
            tree_threads: 1,
            ..NativeOptions::default()
        };
        let warm = self.payload.starts[task]
            .clone()
            .map(WarmStart::from_primal);
        let r = NativeSolver::new(settings).solve(
            &self.model,
            &self.options,
            warm.as_ref(),
            &self.cancel,
        )?;
        let checked = r
            .candidate
            .as_ref()
            .map(|x| check::evaluate(&self.model, x, &self.options.tolerances))
            .transpose()?;
        let feasible = checked.as_ref().map(|e| e.feasible_candidate);
        let residuals = match (&r.candidate, &r.duals) {
            (Some(x), Some(d)) => {
                let k = check::lp_residuals(&self.model, x, d)?;
                json!({"primal":scalar(k.primal),"stationarity":scalar(k.stationarity),"complementarity":scalar(k.complementarity),"dual_sign":scalar(k.dual_sign),"duality_gap":scalar(k.duality_gap)})
            }
            _ => Value::Null,
        };
        if r.status == Status::Optimal && feasible != Some(true) {
            return Err("optimal result failed independent candidate check".into());
        }
        let claim = if r.status == Status::Optimal {
            "numerical_optimum"
        } else if feasible == Some(true) {
            "feasible"
        } else {
            "none"
        };
        let gap = r
            .gap(self.model.sense())?
            .map(|g| json!({"absolute":scalar(g.absolute),"relative":scalar(g.relative)}));
        Ok(TaskResult {
            outcome: Outcome {
                termination: r.status.as_str().into(),
                claim: claim.into(),
                feasible,
                objective: checked.as_ref().map(|e| e.objective),
                x: r.candidate,
                details: json!({"sense":self.model.sense().as_str(),"is_mip":self.model.is_mip(),"best_bound":r.best_bound.map(scalar),"gap":gap,"primal_violation":checked.as_ref().map(|e|scalar(e.primal_violation)),"integrality_violation":checked.as_ref().map(|e|scalar(e.integrality_violation)),"lp_residuals":residuals,"raw_status":r.backend.raw_status,"warm_start":r.backend.start_status.as_str(),"engine_version":r.backend.version,"effective_options":r.backend.effective_options,"message":r.message,"exact_certificate":false}),
            },
            work: [
                ("nodes".into(), r.nodes),
                ("simplex_iterations".into(), r.iterations),
                (
                    "validation_evaluations".into(),
                    u64::from(checked.is_some()),
                ),
            ]
            .into(),
        })
    }
}
fn main() {
    worker::run(Adapter);
}
