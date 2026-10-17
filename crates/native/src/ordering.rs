//! Bounded approximate minimum-degree advice for unsymmetric LU columns.
//! Rows are symbolic clique elements; elimination merges incident elements
//! instead of materializing all pairwise fill edges. Numerical Markowitz
//! selection and threshold pivoting retain the final decision.
use crate::active::Active;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Default)]
pub(crate) struct OrderCache {
    signatures: Vec<u64>,
    pub rank: Vec<usize>,
    pub order: Vec<usize>,
}
impl OrderCache {
    pub fn prepare(&mut self, a: &Active) {
        let m = a.cols.len();
        let signatures: Vec<u64> = a
            .cols
            .iter()
            .map(|col| {
                col.iter().fold(0xcbf29ce484222325u64, |h, e| {
                    (h ^ e.row as u64).wrapping_mul(0x100000001b3)
                })
            })
            .collect();
        let changed = if self.signatures.len() == m {
            signatures
                .iter()
                .zip(&self.signatures)
                .filter(|(a, b)| a != b)
                .count()
        } else {
            m
        };
        if self.order.len() == m && changed <= (m / 8).max(1) {
            return;
        }
        self.order = min_degree(a);
        self.rank.resize(m, 0);
        for (k, &p) in self.order.iter().enumerate() {
            self.rank[p] = k;
        }
        self.signatures = signatures;
    }
}
fn min_degree(a: &Active) -> Vec<usize> {
    let m = a.cols.len();
    let dense_limit = ((m as f64).sqrt() * 10.).max(16.) as usize;
    let mut elements: Vec<Vec<usize>> = a
        .rows
        .iter()
        .filter(|row| row.len() <= dense_limit)
        .map(|row| row.iter().map(|&(p, _)| p as usize).collect())
        .collect();
    let mut incident = vec![Vec::new(); m];
    let mut degree = vec![0usize; m];
    for (e, row) in elements.iter().enumerate() {
        for &p in row {
            incident[p].push(e);
            degree[p] = degree[p].saturating_add(row.len().saturating_sub(1));
        }
    }
    let mut links = incident.iter().map(Vec::len).sum::<usize>();
    let budget = links.saturating_mul(8).max(m.saturating_mul(128));
    let mut live = vec![true; elements.len()];
    let mut eliminated = vec![false; m];
    let mut heap: BinaryHeap<_> = (0..m).map(|p| Reverse((degree[p], p))).collect();
    let mut marked = vec![false; m];
    let mut neighbors = Vec::new();
    let mut order = Vec::with_capacity(m);
    while let Some(Reverse((d, p))) = heap.pop() {
        if eliminated[p] || d != degree[p] {
            continue;
        }
        eliminated[p] = true;
        order.push(p);
        neighbors.clear();
        for &e in &incident[p] {
            if !live[e] {
                continue;
            }
            live[e] = false;
            let decrement = elements[e].len().saturating_sub(1);
            for &q in &elements[e] {
                if eliminated[q] {
                    continue;
                }
                degree[q] = degree[q].saturating_sub(decrement);
                if !marked[q] {
                    marked[q] = true;
                    neighbors.push(q);
                }
            }
        }
        if links.saturating_add(neighbors.len()) > budget {
            let mut rest: Vec<_> = (0..m).filter(|&q| !eliminated[q]).collect();
            rest.sort_unstable_by_key(|&q| (degree[q], q));
            order.extend(rest);
            break;
        }
        neighbors.sort_unstable();
        if !neighbors.is_empty() {
            let e = elements.len();
            let add = neighbors.len() - 1;
            for &q in &neighbors {
                degree[q] = degree[q].saturating_add(add);
                incident[q].push(e);
                marked[q] = false;
                heap.push(Reverse((degree[q], q)));
            }
            links += neighbors.len();
            elements.push(neighbors.clone());
            live.push(true);
        }
    }
    order
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordering_is_deterministic_and_cache_tracks_pattern_changes() {
        let mut a = Active::default();
        a.reset(257);
        for p in 0..257 {
            for i in [p, (p + 1) % 257, (p * 17 + 3) % 257] {
                a.insert(p, i, 1.);
            }
        }
        let mut cache = OrderCache::default();
        cache.prepare(&a);
        let first = cache.order.clone();
        let mut sorted = first.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..257).collect::<Vec<_>>());
        assert_eq!(min_degree(&a), first);
        a.remove(0, 0);
        cache.prepare(&a);
        assert_eq!(cache.order, first);
        a.reset(16);
        for p in 0..16 {
            a.insert(p, p, 1.);
            a.insert(p, 0, 1.);
        }
        cache.prepare(&a);
        let mut sorted = cache.order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..16).collect::<Vec<_>>());
    }
}
