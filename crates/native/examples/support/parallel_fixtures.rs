//! Deterministic connected fixtures with independent known objective values.
//!
//! `cargo run --release -p lp-ip-craft-native --example family_benchmark --
//! <packing|irregular|network|selection> <size> [warm|cold] [dse|dantzig]`
//! prints one JSON record; the solve must reach the known optimum.
use lp_ip_craft::*;

fn csr(n: usize, rows: &[Vec<(usize, f64)>]) -> CsrMatrix {
    let mut indptr = vec![0];
    let mut indices = vec![];
    let mut values = vec![];
    for row in rows {
        let mut row = row.clone();
        row.sort_by_key(|e| e.0);
        for (j, v) in row {
            indices.push(j);
            values.push(v);
        }
        indptr.push(indices.len());
    }
    CsrMatrix::new(rows.len(), n, indptr, indices, values)
}
pub fn packing(n: usize) -> (Model, f64) {
    planted(n, false)
}
pub fn irregular(n: usize) -> (Model, f64) {
    planted(n, true)
}
fn planted(n: usize, irregular: bool) -> (Model, f64) {
    assert!(n >= 8);
    let rows: Vec<Vec<_>> = (0..n)
        .map(|i| vec![(i, 2.), ((i + 1) % n, 1.), ((i + 7) % n, 1.)])
        .collect();
    let rows = if irregular {
        let mut seed = 51921u64;
        (0..n)
            .map(|i| {
                let mut row = std::collections::BTreeMap::from([(i, 8.), ((i + 1) % n, 1.)]);
                for _ in 0..4 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let j = ((seed >> 32) as usize) % n;
                    if j != i {
                        row.insert(j, 1.);
                    }
                }
                row.into_iter().collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    } else {
        rows
    };
    let rhs: Vec<_> = rows.iter().map(|r| r.iter().map(|e| e.1).sum()).collect();
    let mut objective = vec![0.; n];
    for (i, row) in rows.iter().enumerate() {
        let y = (1 + i % 5) as f64;
        for &(j, a) in row {
            objective[j] += y * a;
        }
    }
    // x=1 is feasible, y_i=1+i%5 is dual feasible, with identical objectives.
    let expected = objective.iter().sum();
    let m = Model::new(ModelData {
        sense: Sense::Maximize,
        objective,
        objective_offset: 0.,
        variables: Bounds::new(vec![0.; n], vec![2.; n]),
        kinds: vec![VariableKind::Continuous; n],
        matrix: csr(n, &rows),
        rows: Bounds::new(vec![f64::NEG_INFINITY; n], rhs),
    })
    .unwrap();
    (m, expected)
}
pub fn network(nodes: usize) -> (Model, f64) {
    assert!(nodes >= 3);
    let mut rows = vec![vec![]; nodes];
    let mut objective = vec![];
    for step in 1..=2 {
        for i in 0..nodes - step {
            let j = objective.len();
            rows[i].push((j, 1.));
            rows[i + step].push((j, -1.));
            objective.push(if step == 1 { 1. } else { 3. });
        }
    }
    let n = objective.len();
    let mut balance = vec![0.; nodes];
    balance[0] = 1.;
    balance[nodes - 1] = -1.;
    // Unit flow on consecutive arcs, potentials -i, nonnegative skip-arc RC.
    (
        Model::new(ModelData {
            sense: Sense::Minimize,
            objective,
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![2.; n]),
            kinds: vec![VariableKind::Continuous; n],
            matrix: csr(n, &rows),
            rows: Bounds::fixed(balance),
        })
        .unwrap(),
        (nodes - 1) as f64,
    )
}
pub fn selection(n: usize) -> (Model, f64) {
    let weights: Vec<_> = (0..n).map(|i| 1 + (i * 7 + 3) % 5).collect();
    let profits: Vec<_> = (0..n).map(|i| 1 + (i * 11 + 7) % 17).collect();
    let capacity = weights.iter().sum::<usize>() / 3;
    let mut rows: Vec<Vec<_>> = (0..n).map(|i| vec![(i, 1.), ((i + 1) % n, 1.)]).collect();
    rows.push(
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| (i, w as f64))
            .collect(),
    );
    let mut upper = vec![1.5; n];
    upper.push(capacity as f64 + 0.5);
    // Independent integer dynamic program: first bit, previous bit, capacity.
    let mut expected = 0i64;
    for first in 0..=1 {
        let mut dp = vec![[-1i64; 2]; capacity + 1];
        if first * weights[0] > capacity {
            continue;
        }
        dp[first * weights[0]][first] = (first * profits[0]) as i64;
        for i in 1..n {
            let mut next = vec![[-1; 2]; capacity + 1];
            for (w, states) in dp.iter().enumerate() {
                for (prev, &value) in states.iter().enumerate() {
                    if value < 0 {
                        continue;
                    }
                    next[w][0] = next[w][0].max(value);
                    if prev == 0 && !(i == n - 1 && first == 1) && w + weights[i] <= capacity {
                        next[w + weights[i]][1] =
                            next[w + weights[i]][1].max(value + profits[i] as i64);
                    }
                }
            }
            dp = next;
        }
        expected = expected.max(*dp.iter().flat_map(|r| r.iter()).max().unwrap());
    }
    (
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: profits.iter().map(|&p| p as f64).collect(),
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![1.; n]),
            kinds: vec![VariableKind::Binary; n],
            matrix: csr(n, &rows),
            rows: Bounds::new(vec![f64::NEG_INFINITY; n + 1], upper),
        })
        .unwrap(),
        expected as f64,
    )
}
