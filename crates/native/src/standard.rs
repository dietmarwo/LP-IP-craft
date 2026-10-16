//! Bound substitutions and multiplier postsolve.
use crate::{NativeOptions, dictionary::Dictionary};
use lp_ip_craft::{Duals, Error, Model, Result, Sense};

#[derive(Clone)]
pub(crate) struct Variable {
    pub shift: f64,
    terms: [(usize, f64); 2],
    len: usize,
}
impl Variable {
    fn terms(&self) -> &[(usize, f64)] {
        &self.terms[..self.len]
    }
}
#[derive(Clone)]
pub(crate) struct Standard {
    pub vars: Vec<Variable>,
    pub row_map: Vec<Option<(usize, f64)>>,
    pub objective_scale: f64,
    pub sign: f64,
    pub table: Dictionary,
    pub removed_rows: usize,
}

impl Standard {
    pub fn new(model: &Model, bounds: &lp_ip_craft::Bounds, opts: &NativeOptions) -> Result<Self> {
        let mut vars = Vec::with_capacity(model.n());
        let mut n = 0;
        for j in 0..model.n() {
            let (l, u) = (bounds.lower[j], bounds.upper[j]);
            let (shift, terms, len) = if opts.presolve && l == u {
                (l, [(0, 0.); 2], 0)
            } else if l.is_finite() {
                let k = n;
                n += 1;
                (l, [(k, 1.), (0, 0.)], 1)
            } else if u.is_finite() {
                let k = n;
                n += 1;
                (u, [(k, -1.), (0, 0.)], 1)
            } else {
                let k = n;
                n += 2;
                (0., [(k, 1.), (k + 1, -1.)], 2)
            };
            vars.push(Variable { shift, terms, len });
        }
        let active = crate::presolve::active_rows(
            model,
            bounds,
            opts.presolve
                && opts.redundant_rows
                && model.m() as f64 >= opts.redundant_row_min_ratio * model.n() as f64,
        );
        let removed_rows = active.iter().filter(|&&v| !v).count();
        let row_count = (0..model.m())
            .filter(|&i| active[i])
            .map(|i| {
                usize::from(model.rows().lower[i].is_finite())
                    + usize::from(model.rows().upper[i].is_finite())
            })
            .sum::<usize>()
            + vars
                .iter()
                .enumerate()
                .filter(|(j, v)| {
                    !v.terms().is_empty()
                        && bounds.lower[*j].is_finite()
                        && bounds.upper[*j].is_finite()
                })
                .count();
        if !opts.sparse
            && (row_count + 2)
                .checked_mul(n + 2)
                .is_none_or(|v| v > opts.max_tableau_entries)
        {
            return Err(Error::Unsupported(
                "dense tableau preflight exceeds max_tableau_entries".into(),
            ));
        }
        let sign = if model.sense() == Sense::Maximize {
            1.0
        } else {
            -1.0
        };
        let objective_scale = model
            .objective()
            .iter()
            .fold(1.0_f64, |s, &v| s.max(v.abs()));
        let mut c = vec![0.0; n];
        for (j, v) in vars.iter().enumerate() {
            for &(k, a) in v.terms() {
                c[k] += sign * model.objective()[j] / objective_scale * a;
            }
        }
        let mut table = Dictionary::empty(row_count, &c, opts)?;
        let mut row_map = Vec::with_capacity(row_count);
        let mut count = 0;
        let mut push = |row: &[(usize, f64)],
                        direction: f64,
                        rhs: f64,
                        map: Option<(usize, f64)>|
         -> Result<()> {
            if !rhs.is_finite() || row.iter().any(|&(_, v)| !v.is_finite()) {
                return Err(Error::Evaluation("nonfinite bound transformation".into()));
            }
            if opts.presolve && row.is_empty() && rhs >= 0. {
                return Ok(());
            }
            let scale = row.iter().fold(0.0_f64, |s, &(_, v)| s.max(v.abs()));
            let scale = if scale == 0. { 1. } else { scale };
            let scaled_rhs = rhs / scale;
            if !scaled_rhs.is_finite() {
                return Err(Error::Unsupported("row scaling overflow".into()));
            }
            for &(col, v) in row {
                table.set(count, col, direction * v / scale)?;
            }
            table.set(count, n + 1, scaled_rhs)?;
            row_map.push(map.map(|(i, s)| (i, s / scale)));
            count += 1;
            Ok(())
        };
        let mut row = Vec::new();
        for i in (0..model.m()).filter(|&i| active[i]) {
            row.clear();
            let mut shift = 0.;
            for k in model.matrix().pattern.indptr[i]..model.matrix().pattern.indptr[i + 1] {
                let j = model.matrix().pattern.indices[k];
                let v = model.matrix().values[k];
                shift += v * vars[j].shift;
                if v != 0. {
                    for &(col, a) in vars[j].terms() {
                        row.push((col, v * a));
                    }
                }
            }
            let (l, u) = (model.rows().lower[i], model.rows().upper[i]);
            if u.is_finite() {
                push(&row, 1., u - shift, Some((i, 1.)))?;
            }
            if l.is_finite() {
                push(&row, -1., shift - l, Some((i, -1.)))?;
            }
        }
        for (j, v) in vars.iter().enumerate() {
            let (l, u) = (bounds.lower[j], bounds.upper[j]);
            if !v.terms().is_empty() && l.is_finite() && u.is_finite() {
                push(v.terms(), 1., u - l, None)?;
            }
        }
        table.truncate_rows(count)?;
        Ok(Self {
            vars,
            row_map,
            objective_scale,
            sign,
            table,
            removed_rows,
        })
    }
    pub fn point(&self) -> Vec<f64> {
        let y = self.table.primal();
        self.vars
            .iter()
            .map(|v| v.shift + v.terms().iter().map(|&(k, s)| s * y[k]).sum::<f64>())
            .collect()
    }
    pub fn duals(&self, model: &Model) -> Duals {
        let y = self.table.multipliers();
        let mut rows = vec![0.0; model.m()];
        for (k, map) in self.row_map.iter().enumerate() {
            if let Some((i, s)) = map {
                rows[*i] += self.sign * self.objective_scale * s * y[k];
            }
        }
        let mut at = vec![0.0; model.n()];
        model.matrix().multiply_transpose(&rows, &mut at);
        Duals {
            row_duals: rows,
            reduced_costs: model
                .objective()
                .iter()
                .zip(at)
                .map(|(&c, a)| c - a)
                .collect(),
        }
    }
    pub fn branch(
        &mut self,
        j: usize,
        value: f64,
        upper: bool,
        opts: &NativeOptions,
    ) -> Result<()> {
        let mut a = Vec::new();
        let sign = if upper { 1.0 } else { -1.0 };
        for &(k, v) in self.vars[j].terms() {
            a.push((k, sign * v));
        }
        self.table
            .append(&a, sign * (value - self.vars[j].shift), opts)?;
        self.row_map.push(None);
        Ok(())
    }
}
