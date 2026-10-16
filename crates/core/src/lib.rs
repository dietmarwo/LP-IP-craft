//! Contracts for LP and mixed-integer LP solvers: a validated model, a solver
//! trait, typed results with explicit bound/gap and dual conventions, and
//! solver-independent checks.
//!
//! This crate has no dependencies. Real optimization is in the sibling crate
//! `lp-ip-craft-highs`, an adapter to the open HiGHS solver that implements
//! [`LpIpSolver`]. [`DummySolver`] stays as a contract fixture: it evaluates
//! one candidate and returns it with [`Status::Stub`].
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
//! assert!(result.best_bound.is_none() && result.gap(model.sense())?.is_none());
//! # Ok::<(), lp_ip_craft::Error>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bound;
pub mod check;
mod dummy;
mod error;
mod model;
mod solver;
mod sparse;

pub use bound::{SafeBoundStatus, SafeDualBound, safe_dual_bound};
pub use check::evaluate;
pub use dummy::DummySolver;
pub use error::{Error, Result};
pub use model::{Model, ModelData, Sense, VariableKind};
pub use solver::{
    BackendReport, CancellationToken, Duals, Evaluation, Gap, LpIpSolver, Options, SolveResult,
    StartStatus, Status, Tolerances, WarmStart,
};
pub use sparse::{Bounds, CsrMatrix, CsrPattern};
