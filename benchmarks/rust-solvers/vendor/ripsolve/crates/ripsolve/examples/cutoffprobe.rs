//! How many columns are decided by probing against "beat the incumbent".
//!
//! A reference solver's `rapidlearning` returns two things on `chromaticindex32-8`: 200
//! conflict constraints, which this solver can now produce and which do not move the
//! bound, and **620 global domain reductions**, which it cannot. A domain reduction is a
//! fixing, and a fixing strengthens the relaxation where a clause only forbids a
//! combination.
//!
//! The guess this checks: those come from probing the model *with the objective cutoff
//! row on it*. Presolve probes the plain model, and so does `probe_implications`; neither
//! has ever supposed a value while also demanding a better objective, which is a strictly
//! stronger question and refutes far more often.
use ripsolve::cuts::{Conflicts, Cut};
use ripsolve::lp::Lp;
use ripsolve::{Problem, propagate};

fn main() {
    let path = std::env::args().nth(1).expect("model path");
    let cutoff: f64 = std::env::args().nth(2).expect("cutoff").parse().unwrap();
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    ripsolve::presolve::presolve(&mut problem, 20);

    let objective: Vec<(usize, f64)> = problem
        .obj
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c != 0.0)
        .map(|(j, &c)| (j, c))
        .collect();
    let mut bounded = problem.clone();
    bounded.add_cuts(&[Cut { coefficients: objective, lb: f64::NEG_INFINITY, ub: cutoff }]);

    for (name, model) in [("plain", &problem), ("with cutoff", &bounded)] {
        let rows = model.matrix.to_csr();
        let conflicts = Conflicts::of(model);
        let mut scratch = propagate::Scratch::new(model.n_cols());
        let mut refuted = 0usize;
        let mut both = 0usize;
        let started = std::time::Instant::now();
        for j in 0..model.n_cols() {
            if model.col_ub[j] - model.col_lb[j] < 0.5 {
                continue;
            }
            let mut out = [false, false];
            for (slot, value) in [0.0f64, 1.0].iter().enumerate() {
                let mut lb = model.col_lb.clone();
                let mut ub = model.col_ub.clone();
                lb[j] = *value;
                ub[j] = *value;
                scratch.begin();
                out[slot] = propagate::propagate(
                    model, &rows, &conflicts, &propagate::Clauses::default(),
                    &mut scratch, &mut lb, &mut ub,
                    &[(j as u32, propagate::Seed::Decision)], 20_000, 1e-9,
                )
                .is_err();
            }
            match out {
                [true, true] => both += 1,
                [true, false] | [false, true] => refuted += 1,
                _ => {}
            }
        }
        println!(
            "{name:<12} {refuted} columns decided by refutation, {both} with both values \
             refuted, in {:.2}s",
            started.elapsed().as_secs_f64()
        );
    }
    let root = Lp::relaxation(&problem).solve_with_limit(1_000_000);
    println!("root relaxation {:.6}", root.objective);
}
