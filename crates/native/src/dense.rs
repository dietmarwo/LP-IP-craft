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
        execution: &crate::parallel::Execution,
    ) -> Option<Self> {
        debug_assert!((1..=8).contains(&execution.threads()));
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
            let remaining = n - end;
            let work = remaining
                .saturating_mul(remaining)
                .saturating_mul(end - start);
            if execution.threads() > 1 && work >= 32_768 {
                let (panel, trailing) = d.split_at_mut(end * n);
                execution.chunks_mut(trailing, n * 16, work, |_, rows| {
                    for row in rows.chunks_mut(n) {
                        for j0 in (end..n).step_by(TILE) {
                            let jend = (j0 + TILE).min(n);
                            for t in start..end {
                                let l = row[t];
                                let source = &panel[t * n + j0..t * n + jend];
                                for (v, &u) in row[j0..jend].iter_mut().zip(source) {
                                    *v -= l * u;
                                }
                            }
                        }
                    }
                });
            } else {
                schur_serial(&mut d, n, start, end);
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
            w[i] = subtract_dot(rhs[self.rows[i]], &row[..i], &w[..i]);
        }
        for i in (0..n).rev() {
            let row = &self.data[i * n..(i + 1) * n];
            let v = subtract_dot(w[i], &row[i + 1..], &w[i + 1..]);
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

// Independent accumulators shorten the dependency chain of each triangular
// row. This permits vector arithmetic without reassociation/fast-math flags.
// Short rows keep the scalar path to avoid setup and reduction overhead.
#[inline]
fn subtract_dot(mut value: f64, a: &[f64], x: &[f64]) -> f64 {
    debug_assert_eq!(a.len(), x.len());
    if a.len() < 32 {
        for (&a, &x) in a.iter().zip(x) {
            value -= a * x;
        }
        return value;
    }
    let mut sums = [0.; 4];
    let mut ac = a.chunks_exact(4);
    let mut xc = x.chunks_exact(4);
    for (a, x) in ac.by_ref().zip(xc.by_ref()) {
        sums[0] += a[0] * x[0];
        sums[1] += a[1] * x[1];
        sums[2] += a[2] * x[2];
        sums[3] += a[3] * x[3];
    }
    value -= (sums[0] + sums[1]) + (sums[2] + sums[3]);
    for (&a, &x) in ac.remainder().iter().zip(xc.remainder()) {
        value -= a * x;
    }
    value
}

fn schur_serial(d: &mut [f64], n: usize, start: usize, end: usize) {
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

#[cfg(test)]
mod parallel_tests {
    use super::*;
    #[test]
    fn scalar_and_parallel_factors_permutations_and_solves_match() {
        for n in [193, 385] {
            let mut matrix = crate::active::Active::default();
            matrix.reset(n);
            for j in 0..n {
                for i in 0..n {
                    matrix.insert(
                        j,
                        i,
                        if i == j {
                            2. * n as f64
                        } else {
                            ((i * 37 + j * 19) % 17 + 1) as f64 / 16.
                        },
                    );
                }
            }
            let cols = matrix.cols;
            let mut expected = None;
            for threads in [1, 2, 4, 8] {
                let ex = crate::parallel::Execution::new(threads).unwrap();
                let mut f =
                    DenseTail::factor(&cols, (0..n).rev().collect(), (0..n).collect(), n, &ex)
                        .unwrap();
                let actual = (
                    f.rows.clone(),
                    f.cols.clone(),
                    f.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                );
                if let Some(e) = &expected {
                    assert_eq!(&actual, e, "size{n} width{threads}");
                } else {
                    expected = Some(actual);
                }
                let x: Vec<_> = (0..n).map(|i| ((i * 7) % 13) as f64 / 16.).collect();
                let rhs: Vec<_> = (0..n)
                    .map(|i| (0..n).map(|j| cols[j][i].value * x[j]).sum())
                    .collect();
                let mut out = vec![0.; n];
                f.ftran(&rhs, &mut out);
                assert!(out.iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-10));
                let rhs: Vec<_> = (0..n)
                    .map(|j| (0..n).map(|i| cols[j][i].value * x[i]).sum())
                    .collect();
                f.btran(&rhs, &mut out);
                assert!(out.iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-10));
            }
        }
    }
}
