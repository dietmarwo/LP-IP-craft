//! Deterministic coverage fixtures, independent of solver internals.
use lp_ip_craft::*;
fn matrix(n: usize, rows: &[Vec<(usize, f64)>]) -> CsrMatrix {
    let mut ptr = vec![0];
    let mut ix = vec![];
    let mut val = vec![];
    for row in rows {
        let mut sorted = row.clone();
        sorted.sort_by_key(|e| e.0);
        for (j, a) in sorted { ix.push(j); val.push(a); }
        ptr.push(ix.len());
    }
    CsrMatrix::new(rows.len(), n, ptr, ix, val)
}
fn random(state: &mut u64, modulus: usize) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*state >> 32) as usize) % modulus
}
pub fn packing(n: usize, permuted: bool) -> (Model, f64) {
    assert!(n >= 16);
    let degree = if permuted { 3 } else { 9 };
    let mut seed = 839171;
    let mut permutation: Vec<_> = (0..n).collect();
    if permuted {
        for j in (1..n).rev() { let k = random(&mut seed, j + 1); permutation.swap(j,k); }
    }
    let rows: Vec<Vec<_>> = (0..n).map(|i| {
        let mut row = std::collections::BTreeMap::new();
        for k in 0..degree {
            let j = permutation[(i + k*k + 3*k) % n];
            *row.entry(j).or_insert(0.) += if k == 0 { 4. } else { 1. };
        }
        row.into_iter().collect()
    }).collect();
    let mut c = vec![0.;n];
    let rhs = rows.iter().enumerate().map(|(i,row)| {
        for &(j,a) in row { c[j] += a * (1+i%7) as f64; }
        row.iter().map(|e| e.1).sum()
    }).collect();
    let expected = c.iter().sum();
    (Model::new(ModelData {sense:Sense::Maximize, objective:c, objective_offset:0.,
        variables:Bounds::new(vec![0.;n],vec![2.;n]), kinds:vec![VariableKind::Continuous;n],
        matrix:matrix(n,&rows), rows:Bounds::new(vec![f64::NEG_INFINITY;n],rhs)}).unwrap(), expected)
}
pub fn knapsack(n: usize, integer: bool) -> (Model, f64) {
    let mut seed = 71933 + n as u64;
    let w: Vec<_> = (0..n).map(|_| 1+random(&mut seed,31)).collect();
    let c: Vec<_> = (0..n).map(|_| 1+random(&mut seed,47)).collect();
    let multiplicity = if integer {3} else {1};
    let cap = multiplicity*w.iter().sum::<usize>()/3;
    let mut dp = vec![0usize;cap+1];
    for j in 0..n { for _ in 0..multiplicity { for b in (w[j]..=cap).rev() { dp[b]=dp[b].max(dp[b-w[j]]+c[j]); } } }
    let expected = dp[cap] as f64;
    let row = w.iter().enumerate().map(|(j,&a)|(j,a as f64)).collect();
    (Model::new(ModelData {sense:Sense::Maximize, objective:c.iter().map(|&a|a as f64).collect(), objective_offset:0.,
        variables:Bounds::new(vec![0.;n],vec![multiplicity as f64;n]),
        kinds:vec![if integer {VariableKind::Integer} else {VariableKind::Binary};n],
        matrix:matrix(n,&[row]),rows:Bounds::new(vec![f64::NEG_INFINITY],vec![cap as f64+0.5])}).unwrap(),expected)
}
pub fn binary(n: usize, graph: bool) -> Model {
    assert!(n <= 20);
    let mut seed = 91283+n as u64;
    let c=(0..n).map(|_| (1+random(&mut seed,29)) as f64).collect();
    let mut rows=vec![]; let mut upper=vec![];
    if graph {
        for i in 0..n { for j in i+1..n { if random(&mut seed,7)<2 {rows.push(vec![(i,1.),(j,1.)]);upper.push(1.5);} } }
    } else {
        for _ in 0..5 { let r:Vec<_>=(0..n).map(|j|(j,(1+random(&mut seed,17)) as f64)).collect();
            upper.push(r.iter().map(|e|e.1).sum::<f64>()*0.41);rows.push(r); }
    }
    Model::new(ModelData {sense:Sense::Maximize,objective:c,objective_offset:0.,variables:Bounds::new(vec![0.;n],vec![1.;n]),
        kinds:vec![VariableKind::Binary;n], matrix:matrix(n,&rows),rows:Bounds::new(vec![f64::NEG_INFINITY;rows.len()],upper)}).unwrap()
}

/// Benchmark-only JSON input; null bounds preserve infinity.
pub fn json_file(path: &str) -> (Model, f64) {
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let m = &d["model"];
    let floats = |key: &str, infinity: f64| m[key].as_array().unwrap().iter().map(|v| v.as_f64().unwrap_or(infinity)).collect::<Vec<_>>();
    let ints = |key: &str| m[key].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect::<Vec<_>>();
    let c = floats("objective", f64::NAN);
    let rl = floats("row_lower", f64::NEG_INFINITY);
    let model = Model::new(ModelData {sense:if m["sense"]=="maximize" {Sense::Maximize} else {Sense::Minimize},
        objective_offset:m["objective_offset"].as_f64().unwrap(),
        variables:Bounds::new(floats("lower",f64::NEG_INFINITY),floats("upper",f64::INFINITY)),
        kinds:m["kinds"].as_array().unwrap().iter().map(|v| match v.as_str().unwrap() {"continuous"=>VariableKind::Continuous,"binary"=>VariableKind::Binary,"integer"=>VariableKind::Integer,_=>panic!()}).collect(),
        matrix:CsrMatrix::new(rl.len(),c.len(),ints("indptr"),ints("indices"),floats("values",f64::NAN)),objective:c,
        rows:Bounds::new(rl,floats("row_upper",f64::INFINITY))}).unwrap();
    (model,d["expected"].as_f64().unwrap())
}
