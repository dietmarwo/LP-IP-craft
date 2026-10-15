//! Contracts for LP and mixed-integer LP solvers: a validated model, a solver
//! trait, typed results with explicit bound/gap and dual conventions, and
//! solver-independent checks.
//!
//! **Interface draft. No optimizer is implemented.** The only backend,
//! [`DummySolver`], evaluates one candidate and returns it with
//! [`Status::Stub`]. It exists to exercise the contracts end to end,
//! including through the Python binding, before real backends (HiGHS, SCIP,
//! native components) are integrated.
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
    CancellationToken, Duals, Evaluation, Gap, LpIpSolver, Options, SolveResult, Status,
    Tolerances, WarmStart,
};
pub use sparse::{Bounds, CsrMatrix, CsrPattern};
