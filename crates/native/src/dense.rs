//! Retained dense LU tail, blocked factorization and contiguous triangular solves.
use crate::active::Entry;
const BLOCK: usize = 32;
const TILE: usize = 64;
const ABS_PIVOT: f64 = 1e-11;
pub(super) struct DenseTail {
    pub rows: Vec<usize>,
    pub cols: Vec<usize>,
    pub data: Vec<f64>,
    work: Vec<f64>,
}
impl DenseTail {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn factor(
        cols: &[Vec<Entry>],
        mut rows: Vec<usize>,
        positions: Vec<usize>,
        m: usize,
    ) -> Option<Self> {
        let n = rows.len();
        let mut local = vec![usize::MAX; m];
        for (r, &i) in rows.iter().enumerate() {
            local[i] = r;
        }
        let mut d = vec![0.; n * n];
        for (c, &p) in positions.iter().enumerate() {
            for e in &cols[p] {
                d[local[e.row as usize] * n + c] = e.value;
            }
        }
        for start in (0..n).step_by(BLOCK) {
            let end = (start + BLOCK).min(n);
            {
                let _panel = crate::profile::enter("factor_dense_panel");
                for c in start..end {
                    let mut pivot = c;
                    let mut largest = ABS_PIVOT;
                    for r in c..n {
                        if d[r * n + c].abs() > largest {
                            largest = d[r * n + c].abs();
                            pivot = r;
                        }
                    }
                    if largest == ABS_PIVOT || !largest.is_finite() {
                        return None;
                    }
                    if pivot != c {
                        for j in 0..n {
                            d.swap(c * n + j, pivot * n + j);
                        }
                        rows.swap(c, pivot);
                    }
                    let diagonal = d[c * n + c];
                    for r in c + 1..n {
                        let l = d[r * n + c] / diagonal;
                        d[r * n + c] = l;
                        let (head, tail) = d.split_at_mut(r * n);
                        let source = &head[c * n + c + 1..c * n + end];
                        let target = &mut tail[c + 1..end];
                        for (v, &u) in target.iter_mut().zip(source) {
                            *v -= l * u;
                        }
                    }
                }
                // L11 * U12 = A12, with contiguous row updates.
                for r in start..end {
                    for t in start..r {
                        let l = d[r * n + t];
                        let (head, tail) = d.split_at_mut(r * n);
                        let source = &head[t * n + end..(t + 1) * n];
                        let target = &mut tail[end..n];
                        for (v, &u) in target.iter_mut().zip(source) {
                            *v -= l * u;
                        }
                    }
                }
            }
            let _update = crate::profile::enter("factor_dense_update");
            // A22 -= L21*U12. Short contiguous inner loops are auto-vectorizable.
            for j0 in (end..n).step_by(TILE) {
                let jend = (j0 + TILE).min(n);
                for r0 in (end..n).step_by(TILE) {
                    for r in r0..(r0 + TILE).min(n) {
                        for t in start..end {
                            let l = d[r * n + t];
                            let (head, tail) = d.split_at_mut(r * n);
                            let source = &head[t * n + j0..t * n + jend];
                            let target = &mut tail[j0..jend];
                            for (v, &u) in target.iter_mut().zip(source) {
                                *v -= l * u;
                            }
                        }
                    }
                }
            }
        }
        Some(Self {
            rows,
            cols: positions,
            data: d,
            work: vec![0.; n],
        })
    }
    pub fn ftran(&mut self, rhs: &[f64], out: &mut [f64]) {
        let n = self.len();
        let w = &mut self.work;
        for i in 0..n {
            let row = &self.data[i * n..(i + 1) * n];
            let mut v = rhs[self.rows[i]];
            for (&l, &x) in row[..i].iter().zip(&w[..i]) {
                v -= l * x;
            }
            w[i] = v;
        }
        for i in (0..n).rev() {
            let row = &self.data[i * n..(i + 1) * n];
            let mut v = w[i];
            for (&u, &x) in row[i + 1..].iter().zip(&w[i + 1..]) {
                v -= u * x;
            }
            w[i] = v / row[i];
            out[self.cols[i]] = w[i];
        }
    }
    pub fn btran(&mut self, rhs: &[f64], out: &mut [f64]) {
        let n = self.len();
        for i in 0..n {
            self.work[i] = rhs[self.cols[i]];
        }
        for i in 0..n {
            let row = &self.data[i * n..(i + 1) * n];
            let t = self.work[i] / row[i];
            self.work[i] = t;
            for (v, &u) in self.work[i + 1..].iter_mut().zip(&row[i + 1..]) {
                *v -= u * t;
            }
        }
        for i in (0..n).rev() {
            let t = self.work[i];
            let row = &self.data[i * n..(i + 1) * n];
            for (v, &l) in self.work[..i].iter_mut().zip(&row[..i]) {
                *v -= l * t;
            }
            out[self.rows[i]] = t;
        }
    }
}
