//! Contracts for LP and mixed-integer LP solvers: a validated model, a solver
//! trait, typed results with explicit bound/gap and dual conventions, and
//! solver-independent checks.
//!
//! **Iteration 1.** This dependency-free core retains [`DummySolver`] as a
//! contract fixture. The sibling `lp-ip-craft-highs` crate implements real
//! LP/MIP optimization. Select `backend="highs"` in Python.
//!
//! ```
//! use lp_ip_craft::{
//!     Bounds, CancellationToken, CsrMatrix, DummySolver, LpIpSolver, Model, ModelData,
//!     Options, Sense, Status, VariableKind, WarmStart,
//! };
//!
//! // minimize 2 x0 + 3 x1 + 5  subject to  x0 + x1 = 1,  x binary
//! let model = Model::new(ModelData {
//!     sense: Sense::Minimize,
//!     objective: vec![2.0, 3.0],
//!     objective_offset: 5.0,
//!     variables: Bounds::new(vec![0.0, 0.0], vec![1.0, 1.0]),
//!     kinds: vec![VariableKind::Binary; 2],
//!     matrix: CsrMatrix::from_dense(1, 2, &[1.0, 1.0])?,
//!     rows: Bounds::fixed(vec![1.0]),
//! })?;
//! let start = WarmStart::from_primal(vec![1.0, 0.0]);
//! let result = DummySolver.solve(
//!     &model, &Options::default(), Some(&start), &CancellationToken::default(),
//! )?;
//! assert_eq!(result.status, Status::Stub); // never an optimality claim
//! let evaluation = result.evaluation.as_ref().unwrap();
//! assert_eq!(evaluation.objective, 7.0);
//! assert!(evaluation.feasible_candidate);
//! assert!(result.best_bound.is_none() && result.gap(model.sense()).is_none());
//! # Ok::<(), lp_ip_craft::Error>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod check;
mod dummy;
mod error;
mod model;
mod solver;
mod sparse;

pub use check::evaluate;
pub use dummy::DummySolver;
pub use error::{Error, Result};
pub use model::{Model, ModelData, Sense, VariableKind};
pub use solver::{
    BackendReport, CancellationToken, Duals, Evaluation, Gap, LpIpSolver, Options, SolveResult,
    Status, Tolerances, WarmStart,
};
pub use sparse::{Bounds, CsrMatrix, CsrPattern};
