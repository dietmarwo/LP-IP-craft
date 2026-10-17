//! Forrest–Tomlin updates, in fixed row/column identities.
//!
//! After L, row-eta transformations reduce the basis to U. A replacement
//! inserts its transformed column (the spike) into U, moves the leaving
//! row/column to the end of the pivot order, and eliminates that row.
//! For t = U^-T e_p the elimination multipliers are -U_pp*t_i (i != p).
//! The new diagonal is U_pp * (B^-1 a)_p. Keeping row and column identities
//! fixed makes the cyclic permutation implicit in `order`.
use super::lu::Lists;

pub(crate) struct Ft {
    row_of: Vec<usize>,
    order: Vec<usize>,
    rank: Vec<usize>,
    diag: Vec<f64>,
    rows: Vec<Vec<(usize, f64)>>,
    cols: Vec<Vec<(usize, f64)>>,
    row_etas: Vec<(usize, Vec<(usize, f64)>)>,
    nnz: usize,
    spike: Vec<(usize, f64)>,
    ep: Vec<(usize, f64)>,
    ep_position: Option<usize>,
    scratch: Vec<f64>,
}
impl Ft {
    pub fn new(prow: &[usize], pcol: &[usize], diag: &[f64], u: &Lists) -> Self {
        let m = prow.len();
        let mut f = Self {
            row_of: vec![0; m],
            order: pcol.to_vec(),
            rank: vec![0; m],
            diag: vec![0.; m],
            rows: vec![Vec::new(); m],
            cols: vec![Vec::new(); m],
            row_etas: Vec::new(),
            nnz: 0,
            spike: Vec::new(),
            ep: Vec::new(),
            ep_position: None,
            scratch: vec![0.; m],
        };
        for k in 0..m {
            let p = pcol[k];
            f.row_of[p] = prow[k];
            f.rank[p] = k;
            f.diag[p] = diag[k];
            let (index, value) = u.get(k);
            for (&q, &v) in index.iter().zip(value) {
                if v != 0. {
                    f.rows[p].push((q, v));
                    f.cols[q].push((p, v));
                    f.nnz += 1;
                }
            }
        }
        f
    }
    pub fn entries(&self) -> usize {
        self.diag.len() + self.nnz + self.row_etas.iter().map(|(_, e)| e.len()).sum::<usize>()
    }
    pub fn updates(&self) -> usize {
        self.row_etas.len()
    }
    pub fn ftran(&mut self, v: &mut [f64], out: &mut [f64], capture: bool) {
        {
            let _timer = crate::profile::enter("ftran_ft_rows");
            for &(p, ref eta) in &self.row_etas {
                let mut s = v[self.row_of[p]];
                for &(i, a) in eta {
                    s -= a * v[self.row_of[i]];
                }
                v[self.row_of[p]] = s;
            }
        }
        if capture {
            self.spike.clear();
            for (p, &r) in self.row_of.iter().enumerate() {
                if v[r] != 0. {
                    self.spike.push((p, v[r]));
                }
            }
        }
        for &p in self.order.iter().rev() {
            let t = v[self.row_of[p]] / self.diag[p];
            out[p] = t;
            if t != 0. {
                for &(i, u) in &self.cols[p] {
                    v[self.row_of[i]] -= u * t;
                }
            }
        }
        v.copy_from_slice(out);
    }
    pub fn btran(&mut self, v: &mut [f64], out: &mut [f64], unit: Option<usize>) {
        if unit.is_some() {
            self.ep.clear();
            self.ep_position = unit;
        }
        for &p in &self.order {
            let t = v[p] / self.diag[p];
            out[self.row_of[p]] = t;
            if t != 0. {
                if unit.is_some() {
                    self.ep.push((p, t));
                }
                for &(q, u) in &self.rows[p] {
                    v[q] -= u * t;
                }
            }
        }
        let _timer = crate::profile::enter("btran_ft_rows");
        for &(p, ref eta) in self.row_etas.iter().rev() {
            let t = out[self.row_of[p]];
            if t != 0. {
                for &(i, a) in eta {
                    out[self.row_of[i]] -= a * t;
                }
            }
        }
    }
    /// Return false without mutation if the pivot/spike is unsafe. The
    /// caller records a conventional eta and rebuilds at the next safe point.
    pub fn update(&mut self, p: usize, alpha: f64) -> bool {
        let old = self.diag[p];
        let pivot = old * alpha;
        if !pivot.is_finite() || pivot.abs() < 1e-11 {
            return false;
        }
        if self.ep_position != Some(p) {
            self.ep.clear();
            self.scratch.fill(0.);
            self.scratch[p] = 1.;
            for &i in &self.order[self.rank[p]..] {
                let t = self.scratch[i] / self.diag[i];
                if t != 0. {
                    self.ep.push((i, t));
                    for &(j, u) in &self.rows[i] {
                        self.scratch[j] -= u * t;
                    }
                }
            }
        }
        // Check the separately computed spike/elimination against FTRAN's
        // pivot, before editing either orientation of U.
        self.scratch.fill(0.);
        for &(i, v) in &self.spike {
            self.scratch[i] = v;
        }
        let mut predicted = self.scratch[p];
        let mut scale = predicted.abs();
        for &(i, t) in &self.ep {
            if i == p {
                continue;
            }
            let a = -old * t;
            if !a.is_finite() || a.abs() > 1e10 {
                return false;
            }
            let delta = a * self.scratch[i];
            predicted -= delta;
            scale += delta.abs();
        }
        if !predicted.is_finite()
            || (predicted - pivot).abs() > 1e-8 * scale.max(pivot.abs()) + 1e-12
        {
            return false;
        }
        // Delete the old pivotal row from column lists and its column from rows.
        for &(q, _) in &self.rows[p] {
            let k = self.cols[q].iter().position(|&(i, _)| i == p).unwrap();
            self.cols[q].swap_remove(k);
        }
        self.nnz -= self.rows[p].len();
        self.rows[p].clear();
        for &(i, _) in &self.cols[p] {
            let k = self.rows[i].iter().position(|&(q, _)| q == p).unwrap();
            self.rows[i].swap_remove(k);
        }
        self.nnz -= self.cols[p].len();
        self.cols[p].clear();
        for &(i, v) in &self.spike {
            if i != p {
                self.cols[p].push((i, v));
                self.rows[i].push((p, v));
                self.nnz += 1;
            }
        }
        let eta = self
            .ep
            .iter()
            .filter_map(|&(i, t)| (i != p && t != 0.).then_some((i, -old * t)))
            .collect();
        self.row_etas.push((p, eta));
        self.diag[p] = pivot;
        let k = self.rank[p];
        self.order[k..].rotate_left(1);
        for k in k..self.order.len() {
            self.rank[self.order[k]] = k;
        }
        self.ep_position = None;
        self.spike.clear();
        true
    }
}
