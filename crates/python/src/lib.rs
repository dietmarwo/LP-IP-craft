//! PyO3 binding. The public Python API lives in `python/lp_ip_craft/__init__.py`;
//! this module only converts data and runs the Rust code.
//!
//! There are no Python callbacks: model data is copied into Rust once, when a
//! `Model` is constructed, and every solve or evaluation runs with the
//! interpreter detached.

use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, DummySolver, Error, Evaluation, LpIpSolver, Model,
    ModelData, Options, Sense, Tolerances, VariableKind, WarmStart, evaluate as evaluate_point,
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

fn to_py_err(e: Error) -> PyErr {
    let message = e.message().to_owned();
    match e {
        Error::InvalidInput(_) => PyValueError::new_err(message),
        Error::Evaluation(_) => EvaluationError::new_err(message),
        Error::Unsupported(_) => PyNotImplementedError::new_err(message),
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

/// Runs the dummy backend. Use `lp_ip_craft.solve` instead of calling this directly.
#[pyfunction]
#[pyo3(signature = (model, start, feasibility_tolerance, integrality_tolerance, threads, cancellation, backend="dummy", backend_options=None))]
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
    // Only owned or immutable Rust data crosses into the detached region.
    let r = match backend {
        "dummy" => {
            if backend_options.is_some_and(|d| !d.is_empty()) {
                return Err(PyNotImplementedError::new_err(
                    "dummy does not implement backend options",
                ));
            }
            py.detach(|| DummySolver.solve(inner, &options, start.as_ref(), &token))
                .map_err(to_py_err)?
        }
        "highs" => {
            #[cfg(feature = "highs")]
            {
                let mut settings = lp_ip_craft_highs::HighsOptions::default();
                if let Some(d) = backend_options {
                    for (key, value) in d.iter() {
                        let key: String = key.extract()?;
                        match key.as_str() {
                            "time_limit_seconds" => {
                                settings.time_limit_seconds = value.extract()?
                            }
                            "node_limit" => settings.node_limit = value.extract()?,
                            "iteration_limit" => settings.iteration_limit = value.extract()?,
                            "seed" => settings.seed = value.extract()?,
                            "mip_relative_gap" => settings.mip_relative_gap = value.extract()?,
                            "mip_absolute_gap" => settings.mip_absolute_gap = value.extract()?,
                            "dual_tolerance" => settings.dual_tolerance = value.extract()?,
                            _ => {
                                return Err(PyValueError::new_err(format!(
                                    "unknown HiGHS option: {key}"
                                )));
                            }
                        }
                    }
                }
                py.detach(|| {
                    lp_ip_craft_highs::HighsSolver { settings }.solve(
                        inner,
                        &options,
                        start.as_ref(),
                        &token,
                    )
                })
                .map_err(to_py_err)?
            }
            #[cfg(not(feature = "highs"))]
            {
                return Err(PyNotImplementedError::new_err(
                    "HiGHS support was not compiled",
                ));
            }
        }
        _ => return Err(PyValueError::new_err(format!("unknown backend: {backend}"))),
    };

    let out = PyDict::new(py);
    out.set_item("backend", &r.backend.name)?;
    out.set_item("backend_version", &r.backend.version)?;
    out.set_item("raw_status", &r.backend.raw_status)?;
    out.set_item("start_status", &r.backend.start_status)?;
    out.set_item("total_seconds", r.backend.total_seconds)?;
    out.set_item("conversion_seconds", r.backend.conversion_seconds)?;
    out.set_item("solve_seconds", r.backend.solve_seconds)?;
    out.set_item("check_seconds", r.backend.check_seconds)?;
    let effective = PyDict::new(py);
    for (k, v) in &r.backend.effective_options {
        effective.set_item(k, v)?;
    }
    out.set_item("effective_options", effective)?;
    out.set_item("status", r.status.as_str())?;
    out.set_item("message", &r.message)?;
    out.set_item("candidate", &r.candidate)?;
    out.set_item("best_bound", r.best_bound)?;
    out.set_item("nodes", r.nodes)?;
    out.set_item("iterations", r.iterations)?;
    match r.checked_gap(inner.sense()).map_err(to_py_err)? {
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

/// Independent LP residual checker, including original-sense dual signs.
#[pyfunction]
fn lp_residuals<'py>(
    py: Python<'py>,
    model: &Bound<'py, PyModel>,
    x: &Bound<'py, PyAny>,
    row_duals: &Bound<'py, PyAny>,
    reduced_costs: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let x = floats(x, "x")?;
    let duals = lp_ip_craft::Duals {
        row_duals: floats(row_duals, "row_duals")?,
        reduced_costs: floats(reduced_costs, "reduced_costs")?,
    };
    let inner = &model.get().inner;
    let r = py
        .detach(|| lp_ip_craft::check::lp_residuals(inner, &x, &duals))
        .map_err(to_py_err)?;
    let out = PyDict::new(py);
    out.set_item("primal", r.primal)?;
    out.set_item("stationarity", r.stationarity)?;
    out.set_item("complementarity", r.complementarity)?;
    out.set_item("dual_sign", r.dual_sign)?;
    Ok(out)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<PyModel>()?;
    m.add_class::<Cancellation>()?;
    m.add("EvaluationError", py.get_type::<EvaluationError>())?;
    m.add_function(wrap_pyfunction!(solve_native, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_function(wrap_pyfunction!(lp_residuals, m)?)?;
    Ok(())
}
