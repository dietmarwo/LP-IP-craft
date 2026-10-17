//! PyO3 binding. The public Python API lives in `python/lp_ip_craft/__init__.py`;
//! this module only converts data and runs the Rust code.
//!
//! There are no Python callbacks: model data is copied into Rust once, when a
//! `Model` is constructed, and every solve or evaluation runs with the
//! interpreter detached. The numerical engine is implemented in native Rust.

use lp_ip_craft::{
    BackendReport, Bounds, CancellationToken, CsrMatrix, Duals, DummySolver, Error, Evaluation,
    LpIpSolver, Model, ModelData, Options, Sense, Tolerances, VariableKind, WarmStart, check,
    evaluate as evaluate_point,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString, PyTuple};

create_exception!(
    lp_ip_craft,
    EvaluationError,
    PyRuntimeError,
    "Floating-point evaluation failed, for example a row activity overflowed."
);
create_exception!(
    lp_ip_craft,
    BackendError,
    PyRuntimeError,
    "A native backend failed to set up or run the model."
);

fn to_py_err(e: Error) -> PyErr {
    let message = e.message().to_owned();
    match e {
        Error::InvalidInput(_) => PyValueError::new_err(message),
        Error::Evaluation(_) => EvaluationError::new_err(message),
        Error::Unsupported(_) => PyNotImplementedError::new_err(message),
        Error::Backend(_) => BackendError::new_err(message),
        _ => PyRuntimeError::new_err(e.to_string()),
    }
}

fn reject_text(obj: &Bound<'_, PyAny>, what: &str) -> PyResult<()> {
    if obj.is_instance_of::<PyString>() || obj.is_instance_of::<PyBytes>() {
        return Err(PyTypeError::new_err(format!(
            "{what}: expected a sequence, got {}",
            obj.get_type().name()?
        )));
    }
    Ok(())
}

/// Re-raises `err` with the field name and entry index prepended, keeping its type.
fn at_entry(py: Python<'_>, err: PyErr, what: &str, i: usize) -> PyErr {
    PyErr::from_type(
        err.get_type(py),
        format!("{what}: entry {i}: {}", err.value(py)),
    )
}

/// Copies an iterable of numbers (list, tuple, 1-D NumPy array, ...).
fn floats(obj: &Bound<'_, PyAny>, what: &str) -> PyResult<Vec<f64>> {
    reject_text(obj, what)?;
    let py = obj.py();
    obj.try_iter()?
        .enumerate()
        .map(|(i, item)| {
            item.and_then(|v| v.extract::<f64>())
                .map_err(|e| at_entry(py, e, what, i))
        })
        .collect()
}

/// Copies an iterable of non-negative integers.
fn indices(obj: &Bound<'_, PyAny>, what: &str) -> PyResult<Vec<usize>> {
    reject_text(obj, what)?;
    let py = obj.py();
    obj.try_iter()?
        .enumerate()
        .map(|(i, item)| {
            let value = item
                .and_then(|v| v.extract::<i64>())
                .map_err(|e| at_entry(py, e, what, i))?;
            usize::try_from(value).map_err(|_| {
                PyValueError::new_err(format!("{what}: entry {i} is {value}; must be >= 0"))
            })
        })
        .collect()
}

fn parse_kind(name: &str) -> Option<VariableKind> {
    match name {
        "continuous" => Some(VariableKind::Continuous),
        "integer" => Some(VariableKind::Integer),
        "binary" => Some(VariableKind::Binary),
        _ => None,
    }
}

fn kinds(obj: Option<&Bound<'_, PyAny>>, n: usize) -> PyResult<Vec<VariableKind>> {
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(vec![VariableKind::Continuous; n]);
    };
    reject_text(obj, "kinds")?;
    let py = obj.py();
    obj.try_iter()?
        .enumerate()
        .map(|(i, item)| {
            let name: String = item
                .and_then(|v| v.extract())
                .map_err(|e| at_entry(py, e, "kinds", i))?;
            parse_kind(&name).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "kinds: entry {i} is {name:?}; expected \"continuous\", \"integer\" or \"binary\""
                ))
            })
        })
        .collect()
}

fn parse_sense(name: &str) -> PyResult<Sense> {
    match name {
        "minimize" => Ok(Sense::Minimize),
        "maximize" => Ok(Sense::Maximize),
        _ => Err(PyValueError::new_err(format!(
            "sense is {name:?}; expected \"minimize\" or \"maximize\""
        ))),
    }
}

/// A validated LP or mixed-integer LP; immutable and picklable.
#[pyclass(frozen, module = "lp_ip_craft", name = "Model")]
struct PyModel {
    inner: Model,
}

#[pymethods]
impl PyModel {
    #[new]
    #[pyo3(signature = (
        *, objective, lower, upper, row_lower, row_upper, indptr, indices, values,
        kinds=None, sense="minimize", objective_offset=0.0
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        objective: &Bound<'_, PyAny>,
        lower: &Bound<'_, PyAny>,
        upper: &Bound<'_, PyAny>,
        row_lower: &Bound<'_, PyAny>,
        row_upper: &Bound<'_, PyAny>,
        indptr: &Bound<'_, PyAny>,
        indices: &Bound<'_, PyAny>,
        values: &Bound<'_, PyAny>,
        kinds: Option<&Bound<'_, PyAny>>,
        sense: &str,
        objective_offset: f64,
    ) -> PyResult<Self> {
        let objective = floats(objective, "objective")?;
        let n = objective.len();
        let rows = Bounds::new(
            floats(row_lower, "row_lower")?,
            floats(row_upper, "row_upper")?,
        );
        let data = ModelData {
            sense: parse_sense(sense)?,
            kinds: self::kinds(kinds, n)?,
            variables: Bounds::new(floats(lower, "lower")?, floats(upper, "upper")?),
            matrix: CsrMatrix::new(
                rows.len(),
                n,
                self::indices(indptr, "indptr")?,
                self::indices(indices, "indices")?,
                floats(values, "values")?,
            ),
            rows,
            objective,
            objective_offset,
        };
        let inner = Model::new(data).map_err(to_py_err)?;
        Ok(Self { inner })
    }

    /// Number of variables.
    #[getter]
    fn n(&self) -> usize {
        self.inner.n()
    }

    /// Number of rows.
    #[getter]
    fn m(&self) -> usize {
        self.inner.m()
    }

    /// Number of stored matrix entries.
    #[getter]
    fn nnz(&self) -> usize {
        self.inner.nnz()
    }

    /// Whether any variable is integer or binary.
    #[getter]
    fn is_mip(&self) -> bool {
        self.inner.is_mip()
    }

    #[getter]
    fn sense(&self) -> &'static str {
        self.inner.sense().as_str()
    }

    #[getter]
    fn objective(&self) -> Vec<f64> {
        self.inner.objective().to_vec()
    }

    #[getter]
    fn objective_offset(&self) -> f64 {
        self.inner.objective_offset()
    }

    #[getter]
    fn lower(&self) -> Vec<f64> {
        self.inner.variables().lower.clone()
    }

    #[getter]
    fn upper(&self) -> Vec<f64> {
        self.inner.variables().upper.clone()
    }

    #[getter]
    fn kinds(&self) -> Vec<&'static str> {
        self.inner.kinds().iter().map(|k| k.as_str()).collect()
    }

    #[getter]
    fn row_lower(&self) -> Vec<f64> {
        self.inner.rows().lower.clone()
    }

    #[getter]
    fn row_upper(&self) -> Vec<f64> {
        self.inner.rows().upper.clone()
    }

    #[getter]
    fn indptr(&self) -> Vec<usize> {
        self.inner.matrix().pattern.indptr.clone()
    }

    #[getter]
    fn indices(&self) -> Vec<usize> {
        self.inner.matrix().pattern.indices.clone()
    }

    #[getter]
    fn values(&self) -> Vec<f64> {
        self.inner.matrix().values.clone()
    }

    /// Supports pickling, so a model can be sent to worker processes.
    fn __getnewargs_ex__<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<(Bound<'py, PyTuple>, Bound<'py, PyDict>)> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("objective", self.objective())?;
        kwargs.set_item("lower", self.lower())?;
        kwargs.set_item("upper", self.upper())?;
        kwargs.set_item("row_lower", self.row_lower())?;
        kwargs.set_item("row_upper", self.row_upper())?;
        kwargs.set_item("indptr", self.indptr())?;
        kwargs.set_item("indices", self.indices())?;
        kwargs.set_item("values", self.values())?;
        kwargs.set_item("kinds", self.kinds())?;
        kwargs.set_item("sense", self.sense())?;
        kwargs.set_item("objective_offset", self.objective_offset())?;
        Ok((PyTuple::empty(py), kwargs))
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<PyModel>()
            .is_ok_and(|o| o.get().inner == self.inner)
    }

    fn __repr__(&self) -> String {
        let integral = self
            .inner
            .kinds()
            .iter()
            .filter(|k| k.is_integral())
            .count();
        format!(
            "Model(sense={:?}, n={}, m={}, nnz={}, integral={integral})",
            self.inner.sense().as_str(),
            self.inner.n(),
            self.inner.m(),
            self.inner.nnz()
        )
    }
}

/// Cooperative cancellation flag; checked at the solver's checkpoints.
#[pyclass(frozen, module = "lp_ip_craft")]
#[derive(Default)]
struct Cancellation {
    inner: CancellationToken,
}

#[pymethods]
impl Cancellation {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Idempotent and one-way.
    fn cancel(&self) {
        self.inner.cancel();
    }

    /// Whether cancellation has been requested.
    #[getter]
    fn cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn __repr__(&self) -> String {
        format!("Cancellation(cancelled={})", self.cancelled())
    }
}

fn options(feasibility: f64, integrality: f64, threads: i64) -> PyResult<Options> {
    let threads = usize::try_from(threads)
        .map_err(|_| PyValueError::new_err(format!("threads must be at least 1, got {threads}")))?;
    Ok(Options {
        tolerances: Tolerances {
            feasibility,
            integrality,
        },
        threads,
    })
}

fn evaluation_dict<'py>(py: Python<'py>, e: Evaluation) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("objective", e.objective)?;
    d.set_item("row_activity", e.row_activity)?;
    d.set_item("primal_violation", e.primal_violation)?;
    d.set_item("integrality_violation", e.integrality_violation)?;
    d.set_item("feasible_candidate", e.feasible_candidate)?;
    Ok(d)
}

fn native_settings(
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<lp_ip_craft_native::NativeOptions> {
    fn field<'py, T: for<'a> FromPyObject<'a, 'py>>(
        value: &Bound<'py, PyAny>,
        key: &str,
    ) -> PyResult<T> {
        let py = value.py();
        value.extract::<T>().map_err(|e| {
            let e: PyErr = e.into();
            PyErr::from_type(
                e.get_type(py),
                format!("backend_options.{key}: {}", e.value(py)),
            )
        })
    }
    let mut settings = lp_ip_craft_native::NativeOptions::default();
    let Some(options) = options else {
        return Ok(settings);
    };
    for (key, value) in options.iter() {
        let key: String = key.extract()?;
        match key.as_str() {
            "time_limit_seconds" => settings.time_limit_seconds = field(&value, &key)?,
            "node_limit" => settings.node_limit = field(&value, &key)?,
            "iteration_limit" => settings.iteration_limit = field(&value, &key)?,
            "dual_tolerance" => settings.dual_tolerance = field(&value, &key)?,
            "pivot_tolerance" => settings.pivot_tolerance = field(&value, &key)?,
            "incumbent_heuristic" => settings.incumbent_heuristic = field(&value, &key)?,
            "rational_certification" => settings.rational_certification = field(&value, &key)?,
            "feasibility_jump" => settings.feasibility_jump = field(&value, &key)?,
            "packing_root" => settings.packing_root = field(&value, &key)?,
            "packing_root_iterations" => settings.packing_root_iterations = field(&value, &key)?,
            "packing_root_work" => settings.packing_root_work = field(&value, &key)?,
            "packing_neighborhoods" => settings.packing_neighborhoods = field(&value, &key)?,
            "packing_neighborhood_size" => {
                settings.packing_neighborhood_size = field(&value, &key)?
            }
            "packing_workers" => settings.packing_workers = field(&value, &key)?,
            "packing_neighborhood_nodes" => {
                settings.packing_neighborhood_nodes = field(&value, &key)?
            }
            "packing_cuts" => settings.packing_cuts = field(&value, &key)?,
            "packing_cut_work" => settings.packing_cut_work = field(&value, &key)?,
            "packing_combine_repairs" => settings.packing_combine_repairs = field(&value, &key)?,
            "packing_rounds" => settings.packing_rounds = field(&value, &key)?,
            "packing_neighborhood_growth" => {
                settings.packing_neighborhood_growth = field(&value, &key)?
            }
            "packing_seed_strategy" => settings.packing_seed_strategy = field(&value, &key)?,
            "packing_residual_cliques" => settings.packing_residual_cliques = field(&value, &key)?,
            "packing_clique_work" => settings.packing_clique_work = field(&value, &key)?,
            "packing_price_diagnostics" => {
                settings.packing_price_diagnostics = field(&value, &key)?
            }
            "packing_repair_seconds" => settings.packing_repair_seconds = field(&value, &key)?,
            "packing_proof_seconds" => settings.packing_proof_seconds = field(&value, &key)?,
            "partial_dual_bounds" => settings.partial_dual_bounds = field(&value, &key)?,
            "scalable_branching" => settings.scalable_branching = field(&value, &key)?,
            "branch_probe_candidates" => settings.branch_probe_candidates = field(&value, &key)?,
            "branch_probe_pivots" => settings.branch_probe_pivots = field(&value, &key)?,
            "branch_probe_work" => settings.branch_probe_work = field(&value, &key)?,
            "tree_threads" => settings.tree_threads = field(&value, &key)?,
            "tree_split_depth" => settings.tree_split_depth = field(&value, &key)?,
            "feasibility_jump_steps" => settings.feasibility_jump_steps = field(&value, &key)?,
            "objective_lattice" => settings.objective_lattice = field(&value, &key)?,
            "node_reoptimization" => settings.node_reoptimization = field(&value, &key)?,
            "strong_branching" => settings.strong_branching = field(&value, &key)?,
            "reliability_branching" => settings.reliability_branching = field(&value, &key)?,
            "reliability_threshold" => settings.reliability_threshold = field(&value, &key)?,
            "reliability_max_probes" => settings.reliability_max_probes = field(&value, &key)?,
            "reduced_cost_fixing" => settings.reduced_cost_fixing = field(&value, &key)?,
            "factor_replay" => settings.factor_replay = field(&value, &key)?,
            "mip_diagnostics" => settings.mip_diagnostics = field(&value, &key)?,
            "mip_trace_limit" => settings.mip_trace_limit = field(&value, &key)?,
            "adaptive_pricing" => settings.adaptive_pricing = field(&value, &key)?,
            "devex" => settings.devex = field(&value, &key)?,
            "avoid_dominant_components" => {
                settings.avoid_dominant_components = field(&value, &key)?
            }
            "singleton_presolve" => settings.singleton_presolve = field(&value, &key)?,
            "root_propagation" => settings.root_propagation = field(&value, &key)?,
            "mixed_propagation" => settings.mixed_propagation = field(&value, &key)?,
            "root_cuts" => settings.root_cuts = field(&value, &key)?,
            "root_separation" => settings.root_separation = field(&value, &key)?,
            "root_cut_rounds" => settings.root_cut_rounds = field(&value, &key)?,
            "cover_lifting" => settings.cover_lifting = field(&value, &key)?,
            "steepest_edge" => settings.steepest_edge = field(&value, &key)?,
            "redundant_row_min_ratio" => settings.redundant_row_min_ratio = field(&value, &key)?,
            "redundant_rows" => settings.redundant_rows = field(&value, &key)?,
            "component_threads" => settings.component_threads = field(&value, &key)?,
            "component_parallel_min_entries" => {
                settings.component_parallel_min_entries = field(&value, &key)?
            }
            "decompose" => settings.decompose = field(&value, &key)?,
            "decomposition_min_variables" => {
                settings.decomposition_min_variables = field(&value, &key)?
            }
            "presolve" => settings.presolve = field(&value, &key)?,
            "max_factor_entries" => settings.max_factor_entries = field(&value, &key)?,
            "adaptive_refactor" => settings.adaptive_refactor = field(&value, &key)?,
            "refactor_interval" => settings.refactor_interval = field(&value, &key)?,
            "factor_pivot_search" => settings.factor_pivot_search = field(&value, &key)?,
            "factor_ordering" => {
                settings.factor_ordering = match field::<String>(&value, &key)?.as_str() {
                    "auto" => lp_ip_craft_native::FactorOrdering::Auto,
                    "natural" => lp_ip_craft_native::FactorOrdering::Natural,
                    "min_degree" => lp_ip_craft_native::FactorOrdering::MinDegree,
                    _ => {
                        return Err(PyValueError::new_err(
                            "factor_ordering must be auto, natural, or min_degree",
                        ));
                    }
                }
            }
            "basis_update" => {
                settings.basis_update = match field::<String>(&value, &key)?.as_str() {
                    "auto" => lp_ip_craft_native::BasisUpdate::Auto,
                    "eta" => lp_ip_craft_native::BasisUpdate::Eta,
                    "forrest_tomlin" => lp_ip_craft_native::BasisUpdate::ForrestTomlin,
                    _ => {
                        return Err(PyValueError::new_err(
                            "basis_update must be auto, eta, or forrest_tomlin",
                        ));
                    }
                }
            }
            _ => {
                return Err(PyValueError::new_err(format!(
                    "backend_options: unknown native option {key:?}"
                )));
            }
        }
    }
    Ok(settings)
}

/// Backends compiled into this extension.
#[pyfunction]
fn backends() -> Vec<&'static str> {
    vec!["dummy", "native"]
}

fn report_dict<'py>(py: Python<'py>, r: &BackendReport) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("name", &r.name)?;
    d.set_item("version", &r.version)?;
    d.set_item("raw_status", &r.raw_status)?;
    d.set_item("start_status", r.start_status.as_str())?;
    let effective = PyDict::new(py);
    for (key, value) in &r.effective_options {
        effective.set_item(key, value)?;
    }
    d.set_item("effective_options", effective)?;
    d.set_item("total_seconds", r.total_seconds)?;
    d.set_item("conversion_seconds", r.conversion_seconds)?;
    d.set_item("solve_seconds", r.solve_seconds)?;
    d.set_item("check_seconds", r.check_seconds)?;
    d.set_item("time_budget_exceeded", r.time_budget_exceeded)?;
    Ok(d)
}

/// Runs a backend. Use `lp_ip_craft.solve` instead of calling this directly.
#[pyfunction]
#[pyo3(signature = (model, start, feasibility_tolerance, integrality_tolerance, threads, cancellation, backend, backend_options))]
#[allow(clippy::too_many_arguments)]
fn solve_native<'py>(
    py: Python<'py>,
    model: &Bound<'py, PyModel>,
    start: Option<&Bound<'py, PyAny>>,
    feasibility_tolerance: f64,
    integrality_tolerance: f64,
    threads: i64,
    cancellation: &Bound<'py, Cancellation>,
    backend: &str,
    backend_options: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyDict>> {
    let start = match start.filter(|s| !s.is_none()) {
        Some(s) => Some(WarmStart::from_primal(floats(s, "start")?)),
        None => None,
    };
    let options = options(feasibility_tolerance, integrality_tolerance, threads)?;
    let token = cancellation.get().inner.clone();
    let inner: &Model = &model.get().inner;
    // Only immutable or owned Rust data enters the detached region; a native
    // backend handle is created, used and dropped inside it.
    let r = match backend {
        "dummy" => {
            if backend_options.is_some() {
                return Err(PyNotImplementedError::new_err(
                    "the dummy backend takes no backend_options",
                ));
            }
            py.detach(|| DummySolver.solve(inner, &options, start.as_ref(), &token))
                .map_err(to_py_err)?
        }
        "native" => {
            let mut solver =
                lp_ip_craft_native::NativeSolver::new(native_settings(backend_options)?);
            py.detach(|| solver.solve(inner, &options, start.as_ref(), &token))
                .map_err(to_py_err)?
        }
        _ => {
            return Err(PyValueError::new_err(format!(
                "unknown backend {backend:?}; expected \"dummy\" or \"native\""
            )));
        }
    };

    result_dict(py, inner.sense(), r)
}

fn result_dict(
    py: Python<'_>,
    sense: Sense,
    r: lp_ip_craft::SolveResult,
) -> PyResult<Bound<'_, PyDict>> {
    let out = PyDict::new(py);
    out.set_item("status", r.status.as_str())?;
    out.set_item("message", &r.message)?;
    out.set_item("candidate", &r.candidate)?;
    out.set_item("best_bound", r.best_bound)?;
    out.set_item("nodes", r.nodes)?;
    out.set_item("iterations", r.iterations)?;
    out.set_item("backend", report_dict(py, &r.backend)?)?;
    match r.gap(sense).map_err(to_py_err)? {
        Some(gap) => {
            let d = PyDict::new(py);
            d.set_item("absolute", gap.absolute)?;
            d.set_item("relative", gap.relative)?;
            out.set_item("gap", d)?;
        }
        None => out.set_item("gap", py.None())?,
    }
    match r.duals {
        Some(duals) => {
            let d = PyDict::new(py);
            d.set_item("row_duals", duals.row_duals)?;
            d.set_item("reduced_costs", duals.reduced_costs)?;
            out.set_item("duals", d)?;
        }
        None => out.set_item("duals", py.None())?,
    }
    match r.evaluation {
        Some(e) => out.set_item("evaluation", evaluation_dict(py, e)?)?,
        None => out.set_item("evaluation", py.None())?,
    }
    Ok(out)
}

/// Ordered independent solves; no Python callbacks or model copies in workers.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn solve_batch_native<'py>(
    py: Python<'py>,
    models: Vec<Py<PyModel>>,
    workers: usize,
    feasibility_tolerance: f64,
    integrality_tolerance: f64,
    cancellation: &Bound<'py, Cancellation>,
    backend_options: Option<&Bound<'py, PyDict>>,
) -> PyResult<Vec<Py<PyAny>>> {
    let options = options(feasibility_tolerance, integrality_tolerance, 1)?;
    let solver = lp_ip_craft_native::NativeSolver::new(native_settings(backend_options)?);
    let refs: Vec<&Model> = models.iter().map(|m| &m.bind(py).get().inner).collect();
    let token = cancellation.get().inner.clone();
    let results = py
        .detach(|| solver.solve_batch(&refs, &options, workers, &token))
        .map_err(to_py_err)?;
    // A per-job error is an exception value, so successful jobs remain usable.
    results
        .into_iter()
        .zip(&refs)
        .map(|(r, m)| match r {
            Ok(r) => Ok(result_dict(py, m.sense(), r)?.into_any().unbind()),
            Err(e) => Ok(to_py_err(e).into_value(py).into_any()),
        })
        .collect()
}

/// Independent evaluation of a point. Use `lp_ip_craft.evaluate` instead.
#[pyfunction]
#[pyo3(signature = (model, x, feasibility_tolerance, integrality_tolerance))]
fn evaluate<'py>(
    py: Python<'py>,
    model: &Bound<'py, PyModel>,
    x: &Bound<'py, PyAny>,
    feasibility_tolerance: f64,
    integrality_tolerance: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let x = floats(x, "x")?;
    let tolerances = Tolerances {
        feasibility: feasibility_tolerance,
        integrality: integrality_tolerance,
    };
    let inner: &Model = &model.get().inner;
    let e = py
        .detach(|| evaluate_point(inner, &x, &tolerances))
        .map_err(to_py_err)?;
    evaluation_dict(py, e)
}

/// Independent LP residuals. Use `lp_ip_craft.lp_residuals` instead.
#[pyfunction]
fn lp_residuals<'py>(
    py: Python<'py>,
    model: &Bound<'py, PyModel>,
    x: &Bound<'py, PyAny>,
    row_duals: &Bound<'py, PyAny>,
    reduced_costs: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let x = floats(x, "x")?;
    let duals = Duals {
        row_duals: floats(row_duals, "row_duals")?,
        reduced_costs: floats(reduced_costs, "reduced_costs")?,
    };
    let inner: &Model = &model.get().inner;
    let r = py
        .detach(|| check::lp_residuals(inner, &x, &duals))
        .map_err(to_py_err)?;
    let out = PyDict::new(py);
    out.set_item("primal", r.primal)?;
    out.set_item("stationarity", r.stationarity)?;
    out.set_item("complementarity", r.complementarity)?;
    out.set_item("dual_sign", r.dual_sign)?;
    out.set_item("duality_gap", r.duality_gap)?;
    Ok(out)
}

/// Independent outward-rounded dual bound. Use `lp_ip_craft.safe_dual_bound`.
#[pyfunction]
fn safe_dual_bound<'py>(
    py: Python<'py>,
    model: &Bound<'py, PyModel>,
    row_duals: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let row_duals = floats(row_duals, "row_duals")?;
    let inner: &Model = &model.get().inner;
    let result = py
        .detach(|| lp_ip_craft::safe_dual_bound(inner, &row_duals))
        .map_err(to_py_err)?;
    let out = PyDict::new(py);
    out.set_item("bound", result.bound)?;
    out.set_item("status", result.status.as_str())?;
    out.set_item("index", result.index)?;
    Ok(out)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<PyModel>()?;
    m.add_class::<Cancellation>()?;
    m.add("EvaluationError", py.get_type::<EvaluationError>())?;
    m.add("BackendError", py.get_type::<BackendError>())?;
    m.add_function(wrap_pyfunction!(backends, m)?)?;
    m.add_function(wrap_pyfunction!(solve_native, m)?)?;
    m.add_function(wrap_pyfunction!(solve_batch_native, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_function(wrap_pyfunction!(lp_residuals, m)?)?;
    m.add_function(wrap_pyfunction!(safe_dual_bound, m)?)?;
    Ok(())
}
