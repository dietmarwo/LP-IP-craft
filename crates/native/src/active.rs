//! Cross-indexed active matrix for sparse elimination. Removals are O(1)
//! once either orientation identifies an entry; no dense m-by-m lookup table.
#[derive(Clone, Copy, Debug)]
pub(super) struct Entry {
    pub row: u32,
    pub value: f64,
    row_slot: u32,
}
#[derive(Default)]
pub(super) struct Active {
    pub cols: Vec<Vec<Entry>>,
    pub rows: Vec<Vec<(u32, u32)>>,
    maxima: Vec<f64>,
}
impl Active {
    pub fn reset(&mut self, m: usize) {
        self.cols.resize_with(m, Vec::new);
        self.rows.resize_with(m, Vec::new);
        for col in &mut self.cols {
            col.clear();
        }
        for row in &mut self.rows {
            row.clear();
        }
        self.maxima.resize(m, f64::NAN);
        self.maxima.fill(f64::NAN);
    }
    pub fn insert(&mut self, p: usize, i: usize, value: f64) {
        let col_slot = self.cols[p].len();
        let row_slot = self.rows[i].len();
        self.cols[p].push(Entry {
            row: i as u32,
            value,
            row_slot: row_slot as u32,
        });
        self.rows[i].push((p as u32, col_slot as u32));
        self.maxima[p] = f64::NAN;
    }
    pub fn remove(&mut self, p: usize, at: usize) {
        let e = self.cols[p].swap_remove(at);
        if let Some(moved) = self.cols[p].get(at) {
            self.rows[moved.row as usize][moved.row_slot as usize].1 = at as u32;
        }
        self.rows[e.row as usize].swap_remove(e.row_slot as usize);
        if let Some(&(q, k)) = self.rows[e.row as usize].get(e.row_slot as usize) {
            self.cols[q as usize][k as usize].row_slot = e.row_slot;
        }
        self.maxima[p] = f64::NAN;
    }
    pub fn dirty(&mut self, p: usize) {
        self.maxima[p] = f64::NAN;
    }
    pub fn column_max(&mut self, p: usize) -> f64 {
        if self.maxima[p].is_nan() {
            self.maxima[p] = self.cols[p]
                .iter()
                .fold(0.0_f64, |a, e| a.max(e.value.abs()));
        }
        self.maxima[p]
    }
}

#[derive(Default)]
pub(crate) struct FactorWorkspace {
    pub matrix: Active,
    pub slot: Vec<usize>,
    pub pivot_row: Vec<(usize, f64)>,
    pub pivot_col: Vec<(usize, f64)>,
    pub row_slots: Vec<(u32, u32)>,
    pub input: Vec<(usize, f64)>,
    pub basis_signature: u64,
    pub pivot_cache: Vec<(usize, usize)>,
    pub order_cache: crate::ordering::OrderCache,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(a: &Active) {
        for (p, col) in a.cols.iter().enumerate() {
            for (k, e) in col.iter().enumerate() {
                assert_eq!(
                    a.rows[e.row as usize][e.row_slot as usize],
                    (p as u32, k as u32)
                );
            }
        }
        for (i, row) in a.rows.iter().enumerate() {
            for (s, &(p, k)) in row.iter().enumerate() {
                assert_eq!(a.cols[p as usize][k as usize].row as usize, i);
                assert_eq!(a.cols[p as usize][k as usize].row_slot as usize, s);
            }
        }
    }
    #[test]
    fn cross_indices_survive_insertions_and_swaps() {
        let mut a = Active::default();
        a.reset(31);
        for p in 0..31 {
            for i in 0..31 {
                if (i + p) % 3 == 0 {
                    a.insert(p, i, (i + p + 1) as f64);
                }
            }
        }
        check(&a);
        for round in 0..100 {
            let p = round % 31;
            if !a.cols[p].is_empty() {
                a.remove(p, (round * 7) % a.cols[p].len());
            }
            check(&a);
            let actual = a.cols[p].iter().fold(0.0_f64, |v, e| v.max(e.value.abs()));
            assert_eq!(a.column_max(p), actual);
        }
        a.reset(7);
        check(&a);
        assert!(a.cols.iter().all(Vec::is_empty));
    }
}
