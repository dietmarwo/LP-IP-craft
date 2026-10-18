//! A Python interface shaped like `gurobipy`, for mixed-integer programs.
//!
//! The goal is that a `gurobipy` script that only uses binary variables runs
//! unchanged after swapping the import. So the names, the attribute spellings, and
//! the operator overloading all follow Gurobi's rather than anything more Rust-like:
//!
//! ```python
//! import ripsolve as gp
//! from ripsolve import GRB
//!
//! m = gp.Model("knapsack")
//! x = m.addVars(n, vtype=GRB.BINARY, name="x")
//! m.setObjective(gp.quicksum(value[j] * x[j] for j in range(n)), GRB.MAXIMIZE)
//! m.addConstr(gp.quicksum(weight[j] * x[j] for j in range(n)) <= capacity)
//! m.optimize()
//! print(m.ObjVal, [x[j].X for j in range(n)])
//! ```
//!
//! Binary, general-integer and continuous variables are all supported, so the
//! models this accepts are ordinary MIPs rather than pure BIPs. Callbacks are
//! supported for what they are mostly used for, lazy constraints and user cuts:
//! `optimize(callback)` calls back at `GRB.Callback.MIPNODE` and `MIPSOL`, where
//! `cbGetNodeRel`, `cbGetSolution`, `cbLazy` and `cbCut` work as in gurobipy.
//! Quadratic terms and SOS constraints are not supported.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::{
    PyAttributeError, PyIndexError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple, PyType};

use ripsolve::model::{Problem, RowSense, Sense, VarType};
use ripsolve::cuts::Cut;
use ripsolve::search::{self, Options, Point, Status as SearchStatus};
use ripsolve::sparse::SparseMatrix;

/// A row as the user built it, before translation into a [`Problem`].
struct Row {
    terms: Vec<(usize, f64)>,
    sense: RowSense,
    rhs: f64,
    name: String,
}

/// Everything a model holds, shared between the `Model` and the `Var`s that refer
/// back into it for their solution values.
struct ModelData {
    name: String,
    var_names: Vec<String>,
    var_types: Vec<VarType>,
    objective: Vec<f64>,
    objective_constant: f64,
    sense: Sense,
    rows: Vec<Row>,
    /// Bounds, so a caller can fix a variable the way Gurobi's `.LB`/`.UB` do.
    lower: Vec<f64>,
    upper: Vec<f64>,
    solution: Option<Solved>,
    params: Params,
    /// What the callback being run may read and has added, while one runs.
    callback: Option<CallbackState>,
}

/// One call of the user's callback: where the search is, the point it is at, and the
/// rows the callback has added so far.
struct CallbackState {
    place: i32,
    x: Vec<f64>,
    rows: Vec<Cut>,
}

#[derive(Clone)]
struct Solved {
    status: i32,
    objective: Option<f64>,
    bound: f64,
    values: Vec<f64>,
    nodes: usize,
    runtime: f64,
    gap: f64,
}

#[derive(Clone)]
struct Params {
    time_limit: Option<f64>,
    threads: usize,
    mip_gap: f64,
    output_flag: i32,
    /// Gurobi refuses `cbLazy` unless this is set, and so does this binding, so a
    /// script that runs here runs there.
    lazy_constraints: i32,
    /// Accepted for scripts that set it for `cbCut`. Presolve here never renumbers
    /// the columns a callback sees, so there is nothing for it to change.
    pre_crush: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            time_limit: None,
            threads: 0,
            mip_gap: 0.0,
            output_flag: 1,
            lazy_constraints: 0,
            pre_crush: 0,
        }
    }
}

impl Params {
    /// Set a parameter by Gurobi's name, in any case. Unknown names are rejected
    /// rather than ignored, so a misspelling does not silently do nothing.
    fn set(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        match name.to_ascii_lowercase().as_str() {
            "timelimit" => self.time_limit = Some(value.extract()?),
            "threads" => self.threads = value.extract()?,
            "mipgap" => self.mip_gap = value.extract()?,
            "outputflag" | "logtoconsole" => self.output_flag = value.extract()?,
            "lazyconstraints" => self.lazy_constraints = value.extract()?,
            "precrush" => self.pre_crush = value.extract()?,
            other => {
                return Err(PyKeyError::new_err(format!("unknown parameter {other:?}")));
            }
        }
        Ok(())
    }

    fn get<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        Ok(match name.to_ascii_lowercase().as_str() {
            "timelimit" => self
                .time_limit
                .unwrap_or(f64::INFINITY)
                .into_pyobject(py)?
                .into_any(),
            "threads" => self.threads.into_pyobject(py)?.into_any(),
            "mipgap" => self.mip_gap.into_pyobject(py)?.into_any(),
            "outputflag" | "logtoconsole" => self.output_flag.into_pyobject(py)?.into_any(),
            "lazyconstraints" => self.lazy_constraints.into_pyobject(py)?.into_any(),
            "precrush" => self.pre_crush.into_pyobject(py)?.into_any(),
            other => {
                return Err(PyAttributeError::new_err(format!(
                    "unknown parameter {other:?}"
                )));
            }
        })
    }
}

impl ModelData {
    fn new(name: String) -> Self {
        Self {
            name,
            var_names: Vec::new(),
            var_types: Vec::new(),
            objective: Vec::new(),
            objective_constant: 0.0,
            // Gurobi minimizes unless told otherwise.
            sense: Sense::Minimize,
            rows: Vec::new(),
            lower: Vec::new(),
            upper: Vec::new(),
            solution: None,
            params: Params::default(),
            callback: None,
        }
    }
}

type Shared = Arc<Mutex<ModelData>>;

/// Gurobi's status codes, so `m.Status == GRB.OPTIMAL` compares correctly.
const STATUS_OPTIMAL: i32 = 2;
const STATUS_INFEASIBLE: i32 = 3;
const STATUS_NODE_LIMIT: i32 = 7;
const STATUS_TIME_LIMIT: i32 = 9;

/// Gurobi's callback codes: where the search is, and what `cbGet` may ask.
const WHERE_MIPSOL: i32 = 4;
const WHERE_MIPNODE: i32 = 5;
const MIPSOL_OBJ: i32 = 4002;
const MIPNODE_STATUS: i32 = 5001;

/// `GRB.Callback`, the codes a callback is called with and may ask about.
#[pyclass(name = "Callback", module = "ripsolve", from_py_object)]
#[derive(Clone)]
struct CallbackCodes;

#[pymethods]
impl CallbackCodes {
    /// A new incumbent candidate; `cbGetSolution` and `cbLazy` apply.
    #[classattr]
    const MIPSOL: i32 = WHERE_MIPSOL;
    /// A node's relaxation, the root's included; `cbGetNodeRel`, `cbLazy` and `cbCut`
    /// apply.
    #[classattr]
    const MIPNODE: i32 = WHERE_MIPNODE;
    #[classattr]
    const MIPSOL_OBJ: i32 = MIPSOL_OBJ;
    #[classattr]
    const MIPNODE_STATUS: i32 = MIPNODE_STATUS;
}

/// The `GRB` namespace of constants.
#[pyclass(name = "GRB", module = "ripsolve", from_py_object)]
#[derive(Clone)]
struct Grb;

#[pymethods]
impl Grb {
    #[classattr]
    const BINARY: char = 'B';
    #[classattr]
    const CONTINUOUS: char = 'C';
    #[classattr]
    const INTEGER: char = 'I';
    #[classattr]
    const MINIMIZE: i32 = 1;
    #[classattr]
    const MAXIMIZE: i32 = -1;
    #[classattr]
    const OPTIMAL: i32 = STATUS_OPTIMAL;
    #[classattr]
    const INFEASIBLE: i32 = STATUS_INFEASIBLE;
    #[classattr]
    const NODE_LIMIT: i32 = STATUS_NODE_LIMIT;
    #[classattr]
    const TIME_LIMIT: i32 = STATUS_TIME_LIMIT;
    #[classattr]
    const INFINITY: f64 = f64::INFINITY;
    #[classattr]
    const LESS_EQUAL: char = '<';
    #[classattr]
    const GREATER_EQUAL: char = '>';
    #[classattr]
    const EQUAL: char = '=';

    #[classattr]
    #[allow(non_snake_case)]
    fn Callback(py: Python<'_>) -> Py<PyType> {
        py.get_type::<CallbackCodes>().unbind()
    }
}

/// The spelling a class gives a Gurobi attribute, matched whatever its
/// capitalisation, as gurobipy does: a script may read `model.status` or `v.x` for
/// `Status` and `X`. Only capitalised names are attributes in gurobipy's
/// convention, so methods (`addVar`) stay case-sensitive.
fn canonical_attribute(class: &Bound<'_, PyType>, name: &str) -> PyResult<Option<String>> {
    for entry in class.as_any().dir()? {
        let candidate: String = entry.extract()?;
        if candidate.starts_with(|c: char| c.is_ascii_uppercase())
            && candidate.eq_ignore_ascii_case(name)
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Look up an attribute whatever its capitalisation. Called from `__getattr__`,
/// which Python reaches only when the exact name failed.
fn attribute_ignoring_case<'py>(
    object: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    match canonical_attribute(&object.get_type(), name)? {
        Some(canonical) if canonical != name => object.getattr(canonical.as_str()),
        _ => Err(no_such_attribute(object, name)),
    }
}

fn no_such_attribute(object: &Bound<'_, PyAny>, name: &str) -> PyErr {
    let class = object
        .get_type()
        .qualname()
        .map_or_else(|_| "object".to_string(), |q| q.to_string());
    PyAttributeError::new_err(format!("'{class}' object has no attribute '{name}'"))
}

/// A linear expression: a weighted sum of variables plus a constant.
///
/// Terms are kept in a `BTreeMap` keyed by column so that repeated mentions of a
/// variable accumulate (`x + x` is `2 x`, as in Gurobi) and so the resulting row
/// is deterministic regardless of how the expression was assembled.
#[pyclass(name = "LinExpr", module = "ripsolve", from_py_object)]
#[derive(Clone, Default)]
struct LinExpr {
    terms: BTreeMap<usize, f64>,
    constant: f64,
}

impl LinExpr {
    fn of_var(index: usize) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(index, 1.0);
        Self {
            terms,
            constant: 0.0,
        }
    }

    fn scaled(&self, factor: f64) -> Self {
        Self {
            terms: self.terms.iter().map(|(&j, &v)| (j, v * factor)).collect(),
            constant: self.constant * factor,
        }
    }

    fn add(&self, other: &LinExpr) -> Self {
        let mut out = self.clone();
        for (&j, &v) in &other.terms {
            *out.terms.entry(j).or_insert(0.0) += v;
        }
        out.constant += other.constant;
        out
    }

    /// Interpret a Python object as an expression: a `Var`, a `LinExpr`, or a
    /// number.
    fn extract(value: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        if let Ok(var) = value.extract::<PyRef<Var>>() {
            return Ok(LinExpr::of_var(var.index));
        }
        if let Ok(expr) = value.extract::<PyRef<LinExpr>>() {
            return Ok(expr.clone());
        }
        if let Ok(number) = value.extract::<f64>() {
            return Ok(LinExpr {
                terms: BTreeMap::new(),
                constant: number,
            });
        }
        Err(PyTypeError::new_err(format!(
            "expected a Var, LinExpr or number, got {}",
            value.get_type().name()?
        )))
    }

    /// Build the constraint `self <sense> other`, moving everything to the left.
    fn compare(&self, other: &Bound<'_, PyAny>, sense: RowSense) -> PyResult<TempConstr> {
        let rhs = LinExpr::extract(other)?;
        let combined = self.add(&rhs.scaled(-1.0));
        Ok(TempConstr {
            terms: combined
                .terms
                .into_iter()
                .filter(|&(_, v)| v != 0.0)
                .collect(),
            sense,
            rhs: -combined.constant,
        })
    }
}

#[pymethods]
impl LinExpr {
    #[new]
    #[pyo3(signature = (value = None))]
    fn new(value: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        match value {
            Some(value) => LinExpr::extract(value),
            None => Ok(LinExpr::default()),
        }
    }

    /// The expression's value at the current solution.
    #[getter(getValue)]
    fn get_value_attr(&self) -> f64 {
        self.constant
    }

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.add(&LinExpr::extract(other)?))
    }

    fn __radd__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.add(&LinExpr::extract(other)?))
    }

    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.add(&LinExpr::extract(other)?.scaled(-1.0)))
    }

    fn __rsub__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(LinExpr::extract(other)?.add(&self.scaled(-1.0)))
    }

    fn __mul__(&self, factor: f64) -> LinExpr {
        self.scaled(factor)
    }

    fn __rmul__(&self, factor: f64) -> LinExpr {
        self.scaled(factor)
    }

    fn __neg__(&self) -> LinExpr {
        self.scaled(-1.0)
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.compare(other, RowSense::Le)
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.compare(other, RowSense::Ge)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.compare(other, RowSense::Eq)
    }

    fn __repr__(&self) -> String {
        let mut out = String::new();
        for (j, v) in &self.terms {
            out.push_str(&format!(
                "{}{v} x{j} ",
                if out.is_empty() { "" } else { "+ " }
            ));
        }
        if self.constant != 0.0 || out.is_empty() {
            out.push_str(&format!("+ {}", self.constant));
        }
        format!("<LinExpr: {}>", out.trim())
    }
}

/// A comparison that has not yet been added to a model, as produced by `x + y <= 1`.
#[pyclass(name = "TempConstr", module = "ripsolve", from_py_object)]
#[derive(Clone)]
struct TempConstr {
    terms: Vec<(usize, f64)>,
    sense: RowSense,
    rhs: f64,
}

/// A decision variable.
#[pyclass(name = "Var", module = "ripsolve")]
struct Var {
    data: Shared,
    index: usize,
}

impl Var {
    fn expr(&self) -> LinExpr {
        LinExpr::of_var(self.index)
    }
}

#[pymethods]
impl Var {
    /// The variable's value in the current solution, as Gurobi's `.X`.
    #[getter(X)]
    fn x(&self) -> PyResult<f64> {
        let data = self.data.lock().expect("model lock");
        match &data.solution {
            Some(solved) if solved.objective.is_some() => Ok(solved.values[self.index]),
            _ => Err(PyValueError::new_err(
                "no solution available; call optimize() first and check Status",
            )),
        }
    }

    #[getter(VarName)]
    fn var_name(&self) -> String {
        self.data.lock().expect("model lock").var_names[self.index].clone()
    }

    #[getter(VType)]
    fn vtype(&self) -> char {
        let data = self.data.lock().expect("model lock");
        match data.var_types[self.index] {
            VarType::Continuous => 'C',
            VarType::Integer if data.upper[self.index] <= 1.0 && data.lower[self.index] >= 0.0 => {
                'B'
            }
            VarType::Integer => 'I',
        }
    }

    #[getter(Obj)]
    fn obj(&self) -> f64 {
        self.data.lock().expect("model lock").objective[self.index]
    }

    #[getter(LB)]
    fn lb(&self) -> f64 {
        self.data.lock().expect("model lock").lower[self.index]
    }

    #[setter(LB)]
    fn set_lb(&self, value: f64) {
        self.data.lock().expect("model lock").lower[self.index] = value;
    }

    #[getter(UB)]
    fn ub(&self) -> f64 {
        self.data.lock().expect("model lock").upper[self.index]
    }

    #[setter(UB)]
    fn set_ub(&self, value: f64) {
        self.data.lock().expect("model lock").upper[self.index] = value;
    }

    fn __getattr__<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        attribute_ignoring_case(slf.as_any(), name)
    }

    /// Assignment ignores case too (`v.lb = 0`). Defining `__setattr__` replaces the
    /// generated setters' dispatch, so the writable attributes are routed by hand.
    fn __setattr__(&self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if name.eq_ignore_ascii_case("LB") {
            self.set_lb(value.extract()?);
        } else if name.eq_ignore_ascii_case("UB") {
            self.set_ub(value.extract()?);
        } else {
            return Err(PyAttributeError::new_err(format!(
                "attribute '{name}' of 'Var' cannot be set"
            )));
        }
        Ok(())
    }

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.expr().add(&LinExpr::extract(other)?))
    }

    fn __radd__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.expr().add(&LinExpr::extract(other)?))
    }

    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(self.expr().add(&LinExpr::extract(other)?.scaled(-1.0)))
    }

    fn __rsub__(&self, other: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
        Ok(LinExpr::extract(other)?.add(&self.expr().scaled(-1.0)))
    }

    fn __mul__(&self, factor: f64) -> LinExpr {
        self.expr().scaled(factor)
    }

    fn __rmul__(&self, factor: f64) -> LinExpr {
        self.expr().scaled(factor)
    }

    fn __neg__(&self) -> LinExpr {
        self.expr().scaled(-1.0)
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.expr().compare(other, RowSense::Le)
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.expr().compare(other, RowSense::Ge)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<TempConstr> {
        self.expr().compare(other, RowSense::Eq)
    }

    fn __hash__(&self) -> u64 {
        self.index as u64
    }

    fn __repr__(&self) -> String {
        format!("<Var {}>", self.var_name())
    }
}

/// A constraint that has been added to a model.
#[pyclass(name = "Constr", module = "ripsolve")]
struct Constr {
    data: Shared,
    index: usize,
}

#[pymethods]
impl Constr {
    #[getter(ConstrName)]
    fn constr_name(&self) -> String {
        self.data.lock().expect("model lock").rows[self.index]
            .name
            .clone()
    }

    fn __getattr__<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        attribute_ignoring_case(slf.as_any(), name)
    }

    fn __repr__(&self) -> String {
        format!("<Constr {}>", self.constr_name())
    }
}

/// An optimization model.
// `dict`, so that a script can hang its variables on the model for its callback,
// `model._vars = x`, the way gurobipy's documentation does it.
#[pyclass(name = "Model", module = "ripsolve", dict)]
struct Model {
    data: Shared,
}

impl Model {
    /// Append a row, naming it `R<index>` when the caller gave no name.
    fn push_row(&self, constraint: &TempConstr, name: &str) -> Constr {
        let mut data = self.data.lock().expect("model lock");
        let index = data.rows.len();
        let name = if name.is_empty() {
            format!("R{index}")
        } else {
            name.to_string()
        };
        data.rows.push(Row {
            terms: constraint.terms.clone(),
            sense: constraint.sense,
            rhs: constraint.rhs,
            name,
        });
        data.solution = None;
        Constr {
            data: Arc::clone(&self.data),
            index,
        }
    }

    /// Translate the model as built into the solver's own representation.
    fn to_problem(data: &ModelData) -> PyResult<Problem> {
        let n = data.var_names.len();
        if n == 0 {
            return Err(PyValueError::new_err("model has no variables"));
        }
        let m = data.rows.len();
        let mut triplets = Vec::new();
        let (mut row_lb, mut row_ub, mut row_names) = (Vec::new(), Vec::new(), Vec::new());
        for (i, row) in data.rows.iter().enumerate() {
            for &(j, v) in &row.terms {
                triplets.push((i, j, v));
            }
            let (lo, hi) = row.sense.bounds(row.rhs);
            row_lb.push(lo);
            row_ub.push(hi);
            row_names.push(row.name.clone());
        }

        // The solver always minimizes, so a maximization is negated on the way in
        // and `Problem::objective_value` converts results back.
        let negate = data.sense == Sense::Maximize;
        let flip = |v: f64| if negate { -v } else { v };

        let problem = Problem {
            name: data.name.clone(),
            sense: data.sense,
            obj: data.objective.iter().map(|&c| flip(c)).collect(),
            obj_offset: flip(data.objective_constant),
            matrix: SparseMatrix::from_triplets(m, n, triplets),
            row_lb,
            row_ub,
            col_lb: data.lower.clone(),
            col_ub: data.upper.clone(),
            col_type: data.var_types.clone(),
            col_names: data.var_names.clone(),
            row_names,
        };
        problem
            .validate()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(problem)
    }

    /// The running callback's state, or the error for calling `what` outside one.
    fn in_callback<'a>(data: &'a ModelData, what: &str) -> PyResult<&'a CallbackState> {
        data.callback
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err(format!("{what} is only for use in a callback")))
    }

    fn push_callback_row(data: &mut ModelData, row: &TempConstr) {
        let (lb, ub) = row.sense.bounds(row.rhs);
        if let Some(state) = data.callback.as_mut() {
            state.rows.push(Cut {
                coefficients: row.terms.clone(),
                lb,
                ub,
            });
        }
    }

    /// The point a callback was called at, read for `vars` and shaped like it.
    fn callback_values<'py>(
        &self,
        vars: &Bound<'py, PyAny>,
        place: i32,
        what: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = vars.py();
        let x = {
            let data = self.data.lock().expect("model lock");
            let state = Model::in_callback(&data, what)?;
            if state.place != place {
                return Err(PyRuntimeError::new_err(format!(
                    "{what} is not available at callback code {}",
                    state.place
                )));
            }
            state.x.clone()
        };
        let value = |var: &Bound<'py, PyAny>| -> PyResult<f64> {
            let var = var.extract::<PyRef<Var>>()?;
            x.get(var.index)
                .copied()
                .ok_or_else(|| PyIndexError::new_err("a variable added after optimize began"))
        };
        if vars.extract::<PyRef<Var>>().is_ok() {
            return Ok(value(vars)?.into_pyobject(py)?.into_any());
        }
        if let Ok(dict) = vars.cast::<PyDict>() {
            let out = PyDict::new(py);
            for (key, var) in dict.iter() {
                out.set_item(key, value(&var)?)?;
            }
            return Ok(out.into_any());
        }
        let values: Vec<f64> = vars
            .try_iter()?
            .map(|var| value(&var?))
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, values)?.into_any())
    }

    fn solved(&self) -> PyResult<Solved> {
        self.data
            .lock()
            .expect("model lock")
            .solution
            .clone()
            .ok_or_else(|| PyValueError::new_err("call optimize() first"))
    }
}

// gurobipy's method names, deliberately. Renaming them to Rust convention would
// defeat the entire point of the binding, which is that an existing script runs
// unchanged.
#[allow(non_snake_case)]
#[pymethods]
impl Model {
    #[new]
    #[pyo3(signature = (name = ""))]
    fn new(name: &str) -> Self {
        Self {
            data: Arc::new(Mutex::new(ModelData::new(name.to_string()))),
        }
    }

    /// Add one variable.
    ///
    /// `vtype` follows Gurobi: `GRB.BINARY`, `GRB.INTEGER` or `GRB.CONTINUOUS`,
    /// defaulting to continuous as Gurobi's does. Defaulting to binary instead
    /// would be more convenient here and is exactly the wrong trade: a ported
    /// script calling `addVar()` would silently get a different model.
    ///
    /// Binary is an integer pinned to `[0, 1]`, so the default bounds depend on the
    /// type, as in Gurobi, where an unbounded integer is `[0, inf)`.
    #[pyo3(signature = (obj = 0.0, vtype = 'C', name = "", lb = None, ub = None))]
    fn addVar(
        &self,
        obj: f64,
        vtype: char,
        name: &str,
        lb: Option<f64>,
        ub: Option<f64>,
    ) -> PyResult<Var> {
        let (kind, default_lb, default_ub) = match vtype {
            'B' => (VarType::Integer, 0.0, 1.0),
            'I' => (VarType::Integer, 0.0, f64::INFINITY),
            'C' => (VarType::Continuous, 0.0, f64::INFINITY),
            other => {
                return Err(PyValueError::new_err(format!(
                    "unsupported vtype {other:?}; expected GRB.BINARY, GRB.INTEGER or GRB.CONTINUOUS"
                )));
            }
        };
        let (lb, ub) = (lb.unwrap_or(default_lb), ub.unwrap_or(default_ub));
        if kind == VarType::Integer {
            for bound in [lb, ub] {
                if bound.is_finite() && (bound - bound.round()).abs() > 1e-9 {
                    return Err(PyValueError::new_err(format!(
                        "integer variable has the fractional bound {bound}"
                    )));
                }
            }
        }
        let mut data = self.data.lock().expect("model lock");
        let index = data.var_names.len();
        let name = if name.is_empty() {
            format!("C{index}")
        } else {
            name.to_string()
        };
        data.var_names.push(name);
        data.var_types.push(kind);
        data.objective.push(obj);
        data.lower.push(lb);
        data.upper.push(ub);
        data.solution = None;
        Ok(Var {
            data: Arc::clone(&self.data),
            index,
        })
    }

    /// Add many binary variables, returning a dict keyed as Gurobi's `tupledict` is.
    ///
    /// An integer count gives integer keys `0..n`; an iterable of keys gives those
    /// keys, so `addVars(range(n))` and `addVars(pairs)` both behave as expected.
    // Mirrors gurobipy's signature; splitting it into a struct would make the
    // binding read nothing like the API it exists to imitate.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (keys, obj = 0.0, vtype = 'C', name = "", lb = None, ub = None))]
    fn addVars<'py>(
        &self,
        py: Python<'py>,
        keys: &Bound<'py, PyAny>,
        obj: f64,
        vtype: char,
        name: &str,
        lb: Option<f64>,
        ub: Option<f64>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let key_list: Vec<Py<PyAny>> = if let Ok(count) = keys.extract::<usize>() {
            (0..count)
                .map(|i| i.into_pyobject(py).unwrap().unbind().into())
                .collect()
        } else {
            keys.try_iter()?
                .map(|k| k.map(|k| k.unbind()))
                .collect::<PyResult<_>>()?
        };

        let out = PyDict::new(py);
        for key in key_list {
            let bound = key.bind(py);
            let label = if name.is_empty() {
                String::new()
            } else {
                format!("{name}[{}]", bound.str()?)
            };
            let var = self.addVar(obj, vtype, &label, lb, ub)?;
            out.set_item(bound, Py::new(py, var)?)?;
        }
        Ok(out)
    }

    /// Add a constraint built by comparing expressions, as in `x + y <= 1`.
    #[pyo3(signature = (constraint, name = ""))]
    fn addConstr(&self, constraint: PyRef<TempConstr>, name: &str) -> PyResult<Constr> {
        Ok(self.push_row(&constraint, name))
    }

    /// Add a linear constraint given as `lhs, sense, rhs`, or as a single
    /// comparison in `lhs` with `sense` and `rhs` left out, as gurobipy allows.
    #[pyo3(signature = (lhs, sense = None, rhs = None, name = ""))]
    fn addLConstr(
        &self,
        lhs: &Bound<'_, PyAny>,
        sense: Option<&str>,
        rhs: Option<&Bound<'_, PyAny>>,
        name: &str,
    ) -> PyResult<Constr> {
        let constraint = linear_constraint(lhs, sense, rhs, "addLConstr")?;
        Ok(self.push_row(&constraint, name))
    }

    /// Add several constraints from an iterable of comparisons.
    #[pyo3(signature = (constraints, name = ""))]
    fn addConstrs(&self, constraints: &Bound<'_, PyAny>, name: &str) -> PyResult<usize> {
        let mut added = 0;
        for item in constraints.try_iter()? {
            let item = item?;
            let constraint = item.extract::<PyRef<TempConstr>>()?;
            self.addConstr(constraint, name)?;
            added += 1;
        }
        Ok(added)
    }

    /// Set the objective, and optionally the direction.
    #[pyo3(signature = (expression, sense = None))]
    fn setObjective(&self, expression: &Bound<'_, PyAny>, sense: Option<i32>) -> PyResult<()> {
        let expr = LinExpr::extract(expression)?;
        let mut data = self.data.lock().expect("model lock");
        let n = data.var_names.len();
        data.objective = vec![0.0; n];
        for (&j, &v) in &expr.terms {
            if j >= n {
                return Err(PyIndexError::new_err(
                    "objective refers to an unknown variable",
                ));
            }
            data.objective[j] = v;
        }
        data.objective_constant = expr.constant;
        if let Some(sense) = sense {
            data.sense = if sense < 0 {
                Sense::Maximize
            } else {
                Sense::Minimize
            };
        }
        data.solution = None;
        Ok(())
    }

    /// Set a solver parameter. Unknown names are rejected rather than ignored, so a
    /// misspelling does not silently do nothing.
    fn setParam(&self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.data.lock().expect("model lock").params.set(name, value)
    }

    /// The parameters as attributes, `model.Params.LazyConstraints = 1`.
    #[getter(Params)]
    fn params(&self) -> ParamSet {
        ParamSet {
            data: Arc::clone(&self.data),
        }
    }

    /// Solve the model, calling `callback(model, where)` as the search goes if given.
    ///
    /// The callback runs at `GRB.Callback.MIPNODE`, for the root's and every node's
    /// relaxation, and at `GRB.Callback.MIPSOL`, for every point before it may become
    /// the incumbent. Rows it adds with `cbLazy` or `cbCut` are kept where the point
    /// breaks them. An exception it raises ends its part in the search and is raised
    /// again from here once the search stops.
    #[pyo3(signature = (callback = None))]
    fn optimize(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        callback: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let this = slf.borrow();
        let (problem, params) = {
            let data = this.data.lock().expect("model lock");
            (Model::to_problem(&data)?, data.params.clone())
        };

        let options = Options {
            time_limit: params.time_limit.map(std::time::Duration::from_secs_f64),
            gap_tolerance: params.mip_gap,
            threads: if params.threads == 0 {
                std::thread::available_parallelism().map_or(1, |n| n.get())
            } else {
                params.threads
            },
            ..Options::default()
        };

        // Release the GIL: the solve is pure Rust and can take minutes, and holding
        // the GIL would freeze every other Python thread for its duration.
        let started = std::time::Instant::now();
        let model: Py<Model> = slf.clone().unbind();
        let shared = Arc::clone(&this.data);
        let failure: Mutex<Option<PyErr>> = Mutex::new(None);
        // One callback at a time. The interpreter lock alone does not ensure it: a
        // callback that releases the lock, to sleep or to wait on I/O, would let a
        // second worker's call overwrite the state the first is still reading.
        let turn = Mutex::new(());
        let solution = py.detach(|| match &callback {
            None => search::solve(&problem, options),
            Some(callback) => {
                let separator = |x: &[f64], point: Point| -> Vec<Cut> {
                    let _turn = turn.lock().expect("callback turn");
                    if failure.lock().expect("callback failure").is_some() {
                        return Vec::new();
                    }
                    let place = match point {
                        Point::Candidate => WHERE_MIPSOL,
                        Point::Root | Point::Node => WHERE_MIPNODE,
                    };
                    Python::attach(|py| {
                        shared.lock().expect("model lock").callback = Some(CallbackState {
                            place,
                            x: x.to_vec(),
                            rows: Vec::new(),
                        });
                        let called = callback.call1(py, (model.clone_ref(py), place));
                        let state = shared.lock().expect("model lock").callback.take();
                        match called {
                            Ok(_) => state.map(|s| s.rows).unwrap_or_default(),
                            Err(error) => {
                                *failure.lock().expect("callback failure") = Some(error);
                                Vec::new()
                            }
                        }
                    })
                };
                search::solve_with(&problem, options, None, &separator)
            }
        });
        let runtime = started.elapsed().as_secs_f64();

        let status = match solution.status {
            SearchStatus::Optimal => STATUS_OPTIMAL,
            SearchStatus::Infeasible => STATUS_INFEASIBLE,
            SearchStatus::NodeLimit => STATUS_NODE_LIMIT,
            SearchStatus::TimeLimit => STATUS_TIME_LIMIT,
        };

        let mut data = this.data.lock().expect("model lock");
        data.solution = Some(Solved {
            status,
            objective: solution.objective,
            bound: solution.bound,
            values: solution.x.clone(),
            nodes: solution.nodes,
            runtime,
            gap: solution.gap(),
        });
        drop(data);
        match failure.into_inner().expect("callback failure") {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// A value about the search, inside a callback: `GRB.Callback.MIPSOL_OBJ`, the
    /// candidate's objective, or `GRB.Callback.MIPNODE_STATUS`, which is always
    /// `GRB.OPTIMAL` since a node whose relaxation is not solved is not called back.
    fn cbGet<'py>(&self, py: Python<'py>, what: i32) -> PyResult<Bound<'py, PyAny>> {
        let data = self.data.lock().expect("model lock");
        let state = Model::in_callback(&data, "cbGet")?;
        match (state.place, what) {
            (WHERE_MIPSOL, MIPSOL_OBJ) => {
                let value = data.objective_constant
                    + data
                        .objective
                        .iter()
                        .zip(&state.x)
                        .map(|(c, v)| c * v)
                        .sum::<f64>();
                Ok(value.into_pyobject(py)?.into_any())
            }
            (WHERE_MIPNODE, MIPNODE_STATUS) => Ok(STATUS_OPTIMAL.into_pyobject(py)?.into_any()),
            _ => Err(PyValueError::new_err(format!(
                "cbGet({what}) is not available here"
            ))),
        }
    }

    /// The candidate's values, at `GRB.Callback.MIPSOL`: one for a `Var`, a list for
    /// a list of them, a dict for a dict of them.
    fn cbGetSolution<'py>(&self, vars: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.callback_values(vars, WHERE_MIPSOL, "cbGetSolution")
    }

    /// The node's relaxation values, at `GRB.Callback.MIPNODE`, shaped as
    /// [`Model::cbGetSolution`]'s are.
    fn cbGetNodeRel<'py>(&self, vars: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.callback_values(vars, WHERE_MIPNODE, "cbGetNodeRel")
    }

    /// Add a lazy constraint: a row the model needs, given as `addLConstr` takes one.
    /// Needs `LazyConstraints` set, as in Gurobi.
    #[pyo3(signature = (lhs, sense = None, rhs = None))]
    fn cbLazy(
        &self,
        lhs: &Bound<'_, PyAny>,
        sense: Option<&str>,
        rhs: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let row = linear_constraint(lhs, sense, rhs, "cbLazy")?;
        let mut data = self.data.lock().expect("model lock");
        if data.params.lazy_constraints == 0 {
            return Err(PyRuntimeError::new_err(
                "cbLazy needs the LazyConstraints parameter set to 1",
            ));
        }
        Model::in_callback(&data, "cbLazy")?;
        Model::push_callback_row(&mut data, &row);
        Ok(())
    }

    /// Add a cut: a row every solution satisfies, offered to tighten a node's
    /// relaxation. Only at `GRB.Callback.MIPNODE`, as in Gurobi.
    #[pyo3(signature = (lhs, sense = None, rhs = None))]
    fn cbCut(
        &self,
        lhs: &Bound<'_, PyAny>,
        sense: Option<&str>,
        rhs: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let row = linear_constraint(lhs, sense, rhs, "cbCut")?;
        let mut data = self.data.lock().expect("model lock");
        if Model::in_callback(&data, "cbCut")?.place != WHERE_MIPNODE {
            return Err(PyRuntimeError::new_err(
                "cbCut is only available at GRB.Callback.MIPNODE",
            ));
        }
        Model::push_callback_row(&mut data, &row);
        Ok(())
    }

    /// Every variable, in the order they were added.
    fn getVars(&self, py: Python<'_>) -> PyResult<Vec<Py<Var>>> {
        let count = self.data.lock().expect("model lock").var_names.len();
        (0..count)
            .map(|index| {
                Py::new(
                    py,
                    Var {
                        data: Arc::clone(&self.data),
                        index,
                    },
                )
            })
            .collect()
    }

    /// Write the model to an LP file.
    fn write(&self, path: &str) -> PyResult<()> {
        let data = self.data.lock().expect("model lock");
        let problem = Model::to_problem(&data)?;
        std::fs::write(path, problem.to_lp())
            .map_err(|e| PyValueError::new_err(format!("writing {path}: {e}")))
    }

    #[getter(ObjVal)]
    fn obj_val(&self) -> PyResult<f64> {
        self.solved()?
            .objective
            .ok_or_else(|| PyValueError::new_err("no solution was found"))
    }

    #[getter(ObjBound)]
    fn obj_bound(&self) -> PyResult<f64> {
        Ok(self.solved()?.bound)
    }

    #[getter(Status)]
    fn status(&self) -> PyResult<i32> {
        Ok(self.solved()?.status)
    }

    #[getter(SolCount)]
    fn sol_count(&self) -> PyResult<usize> {
        Ok(usize::from(self.solved()?.objective.is_some()))
    }

    #[getter(NodeCount)]
    fn node_count(&self) -> PyResult<f64> {
        Ok(self.solved()?.nodes as f64)
    }

    #[getter(Runtime)]
    fn runtime(&self) -> PyResult<f64> {
        Ok(self.solved()?.runtime)
    }

    #[getter(MIPGap)]
    fn mip_gap(&self) -> PyResult<f64> {
        Ok(self.solved()?.gap)
    }

    #[getter(NumVars)]
    fn num_vars(&self) -> usize {
        self.data.lock().expect("model lock").var_names.len()
    }

    #[getter(NumConstrs)]
    fn num_constrs(&self) -> usize {
        self.data.lock().expect("model lock").rows.len()
    }

    #[getter(ModelName)]
    fn model_name(&self) -> String {
        self.data.lock().expect("model lock").name.clone()
    }

    #[getter(ModelSense)]
    fn model_sense(&self) -> i32 {
        match self.data.lock().expect("model lock").sense {
            Sense::Minimize => 1,
            Sense::Maximize => -1,
        }
    }

    /// Besides its own attributes, a gurobipy model answers a variable or constraint
    /// attribute with that attribute of every variable or constraint, in order:
    /// `model.X` is the whole solution vector.
    fn __getattr__<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(canonical) = canonical_attribute(&slf.get_type(), name)?
            && canonical != name
        {
            return slf.getattr(canonical.as_str());
        }
        let model = slf.borrow();
        let objects: Vec<Bound<'py, PyAny>> =
            if let Some(canonical) = canonical_attribute(&py.get_type::<Var>(), name)? {
                model
                    .getVars(py)?
                    .into_iter()
                    .map(|var| var.into_bound(py).getattr(canonical.as_str()))
                    .collect::<PyResult<_>>()?
            } else if let Some(canonical) = canonical_attribute(&py.get_type::<Constr>(), name)? {
                (0..model.num_constrs())
                    .map(|index| {
                        let data = Arc::clone(&model.data);
                        Bound::new(py, Constr { data, index })?.getattr(canonical.as_str())
                    })
                    .collect::<PyResult<_>>()?
            } else {
                return Err(no_such_attribute(slf.as_any(), name));
            };
        Ok(PyList::new(py, objects)?.into_any())
    }

    fn __repr__(&self) -> String {
        format!(
            "<ripsolve.Model {:?}: {} vars, {} constrs>",
            self.model_name(),
            self.num_vars(),
            self.num_constrs()
        )
    }
}

/// A constraint given as `lhs, sense, rhs`, or as a single comparison in `lhs` with
/// `sense` and `rhs` left out, as gurobipy's `addLConstr` and `cbLazy` take one.
fn linear_constraint(
    lhs: &Bound<'_, PyAny>,
    sense: Option<&str>,
    rhs: Option<&Bound<'_, PyAny>>,
    what: &str,
) -> PyResult<TempConstr> {
    match (sense, rhs) {
        (None, None) => Ok(lhs.extract::<PyRef<TempConstr>>()?.clone()),
        (Some(sense), Some(rhs)) => {
            let sense = match sense {
                "<" | "<=" => RowSense::Le,
                ">" | ">=" => RowSense::Ge,
                "=" | "==" => RowSense::Eq,
                other => {
                    return Err(PyValueError::new_err(format!(
                        "unknown constraint sense {other:?}"
                    )));
                }
            };
            LinExpr::extract(lhs)?.compare(rhs, sense)
        }
        _ => Err(PyTypeError::new_err(format!(
            "{what} needs both sense and rhs, or neither"
        ))),
    }
}

/// `model.Params`: the parameters as attributes, by Gurobi's names in any case.
#[pyclass(name = "ParamSet", module = "ripsolve")]
struct ParamSet {
    data: Shared,
}

#[pymethods]
impl ParamSet {
    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        self.data.lock().expect("model lock").params.get(py, name)
    }

    fn __setattr__(&self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.data.lock().expect("model lock").params.set(name, value)
    }
}

/// Sum an iterable of variables or expressions.
#[pyfunction]
fn quicksum(terms: &Bound<'_, PyAny>) -> PyResult<LinExpr> {
    let mut total = LinExpr::default();
    for item in terms.try_iter()? {
        total = total.add(&LinExpr::extract(&item?)?);
    }
    Ok(total)
}

/// Read a model from an LP or MPS file.
#[pyfunction]
fn read(py: Python<'_>, path: &str) -> PyResult<Py<Model>> {
    let problem = Problem::from_file(std::path::Path::new(path))
        .map_err(|e| PyValueError::new_err(format!("reading {path}: {e}")))?;

    let model = Model::new(&problem.name);
    {
        let mut data = model.data.lock().expect("model lock");
        data.sense = problem.sense;
        data.var_names = problem.col_names.clone();
        // `Problem` holds the objective in minimization form; the Python side keeps
        // it as the user would have written it.
        let flip = |v: f64| {
            if problem.sense == Sense::Maximize {
                -v
            } else {
                v
            }
        };
        data.objective = problem.obj.iter().map(|&c| flip(c)).collect();
        data.objective_constant = flip(problem.obj_offset);
        data.lower = problem.col_lb.clone();
        data.upper = problem.col_ub.clone();
        data.var_types = problem.col_type.clone();

        let csr = problem.matrix.to_csr();
        for i in 0..problem.n_rows() {
            let (cols, vals) = csr.column(i);
            let terms: Vec<(usize, f64)> = cols.iter().copied().zip(vals.iter().copied()).collect();
            let (lo, hi) = (problem.row_lb[i], problem.row_ub[i]);
            let (sense, rhs) = if lo == hi {
                (RowSense::Eq, lo)
            } else if hi.is_finite() {
                (RowSense::Le, hi)
            } else {
                (RowSense::Ge, lo)
            };
            data.rows.push(Row {
                terms,
                sense,
                rhs,
                name: problem.row_names[i].clone(),
            });
        }
    }
    Py::new(py, model)
}

// Named differently from the crate so the import does not shadow it; `#[pyo3(name)]`
// is what Python sees.
#[pymodule]
#[pyo3(name = "ripsolve")]
fn ripsolve_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Model>()?;
    module.add_class::<Var>()?;
    module.add_class::<Constr>()?;
    module.add_class::<LinExpr>()?;
    module.add_class::<TempConstr>()?;
    module.add_class::<Grb>()?;
    module.add_class::<CallbackCodes>()?;
    module.add_class::<ParamSet>()?;
    module.add_function(wrap_pyfunction!(quicksum, module)?)?;
    module.add_function(wrap_pyfunction!(read, module)?)?;
    module.add("GRB", module.py().get_type::<Grb>())?;
    let _ = PyTuple::empty(module.py());
    Ok(())
}
