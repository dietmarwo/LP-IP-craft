//! A branch-and-cut solver for mixed-integer programs, in pure Rust.
//!
//! Columns may be binary, general integer, or continuous. Every node of the search
//! solves a bounded LP relaxation with the simplex method, and the dual bound that
//! comes back, strengthened by presolve and cutting planes, is what prunes the tree.
//!
//! The solver targets small and medium models, up to roughly a thousand rows. In that
//! range it is competitive with the open-source solvers and with commercial ones.
//! Above it, reach for something else.
//!
//! # Solving a model from a file
//!
//! ```no_run
//! use ripsolve::{Problem, search};
//! use std::path::Path;
//!
//! let problem = Problem::from_file(Path::new("model.lp"))?;
//! problem.validate()?;
//!
//! let solution = search::solve(&problem, search::Options::default());
//! println!("{:?} {:?}", solution.status, solution.objective);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Both LP and MPS are read, chosen by file extension, or explicitly with
//! [`Problem::from_file_as`].
//!
//! # Building a model
//!
//! [`Builder`] assembles a model a column and a row at a time. Objective coefficients
//! are written in the sense you ask for, and the conversion to the solver's internal
//! minimization form happens on `build`.
//!
//! Maximize `3b + 2n` subject to `2b + n <= 12`, with `b` binary and `n` integer in
//! `[0, 10]`:
//!
//! ```
//! use ripsolve::model::{Builder, RowSense, Sense};
//! use ripsolve::search;
//!
//! let mut model = Builder::new(Sense::Maximize).named("example");
//! let b = model.binary("b");
//! let n = model.integer("n", 0.0, 10.0);
//! model.objective(&[(b, 3.0), (n, 2.0)]);
//! model.row(&[(b, 2.0), (n, 1.0)], RowSense::Le, 12.0);
//!
//! let problem = model.build();
//! problem.validate()?;
//!
//! let solution = search::solve(&problem, search::Options::default());
//! assert_eq!(solution.objective, Some(23.0));
//! # Ok::<(), ripsolve::ModelError>(())
//! ```
//!
//! A binary column is an integer column bounded to `[0, 1]`. Nothing in the solver
//! treats that as a distinct case, because branching splits a range and degenerates to
//! fixing at 0 or 1 on its own.
//!
//! [`Builder::continuous`] adds a column with no integrality requirement, and
//! [`Builder::range`] adds a row bounded on both sides, `lb <= a\'x <= ub`, which is the
//! form rows are held in internally.
//!
//! [`Problem`] is a plain struct with public fields, so it can also be filled in
//! directly when a model is being translated from somewhere else. Doing so means
//! keeping the parallel vectors consistent and writing the objective already negated
//! for a maximization, which is what the builder exists to avoid.
//!
//! # Controlling the search
//!
//! [`search::Options`] carries the limits and the tuning. The library defaults to a
//! single thread so that behaviour is reproducible; the command-line application
//! defaults to the machine's parallelism.
//!
//! ```no_run
//! use ripsolve::search::Options;
//! use std::time::Duration;
//!
//! let options = Options {
//!     threads: 8,
//!     time_limit: Some(Duration::from_secs(60)),
//!     gap_tolerance: 0.01,
//!     ..Options::default()
//! };
//! ```
//!
//! A run that stops early reports [`search::Status::NodeLimit`] or
//! [`search::Status::TimeLimit`] with the best solution found and the remaining gap,
//! rather than failing. [`search::Status::Optimal`] is the only status that claims
//! proof.
//!
//! # Where the pieces live
//!
//! [`model`] holds the problem representation, [`reader`] the LP and MPS readers, and
//! [`search`] the branch-and-cut driver. [`lp`] is the simplex: [`lp::Lp`] is a
//! relaxation and the solves against it. [`presolve`], [`cuts`], [`branch`] and
//! [`heuristic`] are the components the search drives, exposed because they are useful
//! to measure on their own.
//!
//! The reasoning behind the defaults, including the approaches that were measured and
//! rejected, is in [design-notes.md](https://github.com/ripsolve/ripsolve/blob/main/docs/design-notes.md).

pub mod aggregate;
pub mod branch;
pub mod compact;
pub mod cdcl;
pub mod cuts;
pub mod generate;
pub mod heuristic;
pub mod lp;
pub mod model;
pub mod presolve;
pub mod propagate;
pub mod reader;
pub mod search;
pub mod sparse;
pub mod symmetry;
mod writer;

pub use model::{Builder, ModelError, Problem, RowSense, Sense, VarType};
pub use reader::{Format, ReadError};
pub use sparse::SparseMatrix;
