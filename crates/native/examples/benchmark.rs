//! Fixed release-mode fixtures; model construction is outside the timer.
use lp_ip_craft::*;
use lp_ip_craft_native::*;
use std::time::Instant;
fn fixture(name: &str) -> Model {
    let (n, m) = match name {
        "fixed" => (220, 35),
        "pivot" => (110, 65),
        "knapsack" => (16, 1),
        "partition" => (12, 3),
        "nodecopy" => (20, 8),
        "redundant" => (120, 300),
        "blocks" => (512, 256),
        _ => panic!(),
    };
    let mip = matches!(name, "knapsack" | "partition" | "nodecopy");
    let mut seed = 21907u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        1. + ((seed >> 32) % 19) as f64
    };
    let c: Vec<_> = (0..n).map(|_| next()).collect();
    let mut a = vec![0.; n * m];
    let mut b = vec![0.; m];
    for i in 0..m {
        for j in 0..n {
            let v = if name == "blocks" {
                if i / 32 == j / 64 { next() } else { 0. }
            } else if name == "partition" {
                if j % 3 == i { 1. } else { 0. }
            } else {
                next()
            };
            a[i * n + j] = v;
            b[i] += v * if mip { 0.43 } else { 0.7 };
        }
    }
    if name == "redundant" {
        for v in &mut b {
            *v *= 4.;
        }
    }
    let lower = vec![0.; n];
    let mut upper = vec![if mip { 1. } else { 2. }; n];
    if name == "fixed" {
        for u in &mut upper[..180] {
            *u = 0.;
        }
    }
    Model::new(ModelData {
        sense: Sense::Maximize,
        objective: c,
        objective_offset: 0.,
        variables: Bounds::new(lower, upper),
        kinds: vec![
            if mip {
                VariableKind::Binary
            } else {
                VariableKind::Continuous
            };
            n
        ],
        matrix: CsrMatrix::from_dense(m, n, &a).unwrap(),
        rows: Bounds::new(vec![f64::NEG_INFINITY; m], b),
    })
    .unwrap()
}
fn main() {
    let repeats: usize = std::env::args()
        .nth(1)
        .unwrap_or("7".into())
        .parse()
        .unwrap();
    let component_threads = std::env::args().nth(2).map(|s| s.parse::<usize>().unwrap());
    println!("[");
    let mut first = true;
    for name in [
        "fixed",
        "pivot",
        "knapsack",
        "partition",
        "nodecopy",
        "redundant",
        "blocks",
    ] {
        let model = fixture(name);
        for run in 0..=repeats {
            let t = Instant::now();
            let mut settings = NativeOptions::default();
            if let Some(t) = component_threads {
                settings.component_threads = t;
            }
            let r = NativeSolver::new(settings)
                .solve(
                    &model,
                    &Options::default(),
                    None,
                    &CancellationToken::default(),
                )
                .unwrap();
            let seconds = t.elapsed().as_secs_f64();
            assert_eq!(r.status, Status::Optimal, "{name}: {r:?}");
            if run > 0 {
                if !first {
                    println!(",");
                }
                first = false;
                print!(
                    "{{\"fixture\":\"{name}\",\"run\":{run},\"seconds\":{seconds},\"nodes\":{},\"pivots\":{},\"objective\":{},\"components\":{},\"workers\":{}}}",
                    r.nodes,
                    r.iterations,
                    r.evaluation.unwrap().objective,
                    r.backend
                        .effective_options
                        .iter()
                        .find(|(k, _)| k == "diagnostics.component_count")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("1"),
                    r.backend
                        .effective_options
                        .iter()
                        .find(|(k, _)| k == "diagnostics.component_workers")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("1")
                );
            }
        }
    }
    println!("\n]");
}
