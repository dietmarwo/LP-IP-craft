//! Explicit numerical-kernel choice; sparse never falls back to a tableau.
use crate::{Budget, NativeOptions, revised::Revised, tableau::Tableau};
use lp_ip_craft::{Result, Status};
#[derive(Clone)]
pub(crate) enum Dictionary {
    Dense(Box<Tableau>),
    Sparse(Box<Revised>),
}
impl Dictionary {
    pub fn empty(m: usize, c: &[f64], opts: &NativeOptions) -> Result<Self> {
        Ok(if opts.sparse {
            Self::Sparse(Box::new(Revised::empty(m, c, opts.max_sparse_entries)))
        } else {
            Self::Dense(Box::new(Tableau::empty(m, c, opts)?))
        })
    }
    pub fn set(&mut self, i: usize, j: usize, v: f64) -> Result<()> {
        match self {
            Self::Dense(t) => {
                t.set(i, j, v);
                Ok(())
            }
            Self::Sparse(t) => t.set(i, j, v),
        }
    }
    pub fn truncate_rows(&mut self, m: usize) -> Result<()> {
        match self {
            Self::Dense(t) => {
                t.truncate_rows(m);
                Ok(())
            }
            Self::Sparse(t) => t.finish(m),
        }
    }
    pub fn solve(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        match self {
            Self::Dense(t) => t.solve(budget, eps),
            Self::Sparse(t) => t.solve(budget, eps),
        }
    }
    pub fn dual_simplex(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        match self {
            Self::Dense(t) => t.dual_simplex(budget, eps),
            Self::Sparse(t) => t.dual_simplex(budget, eps),
        }
    }
    pub fn primal(&self) -> Vec<f64> {
        match self {
            Self::Dense(t) => t.primal(),
            Self::Sparse(t) => t.primal(),
        }
    }
    pub fn multipliers(&self) -> Vec<f64> {
        match self {
            Self::Dense(t) => t.multipliers(),
            Self::Sparse(t) => t.multipliers(),
        }
    }
    pub fn append(&mut self, row: &[(usize, f64)], rhs: f64, opts: &NativeOptions) -> Result<()> {
        match self {
            Self::Dense(t) => {
                let mut a = vec![0.; t.n];
                for &(j, v) in row {
                    a[j] = v;
                }
                t.append(&a, rhs, opts)
            }
            Self::Sparse(t) => t.append(row, rhs),
        }
    }
    pub fn diagnostics(&self) -> (usize, u64, u64) {
        match self {
            Self::Dense(_) => (0, 0, 0),
            Self::Sparse(t) => (t.peak_entries, t.refactors, t.updates),
        }
    }
}
