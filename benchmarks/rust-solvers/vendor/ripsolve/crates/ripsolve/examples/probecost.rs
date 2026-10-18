//! What probing costs, and what it buys, on one model at a time.
//!
//! Probing is the expensive half of presolve, and the reason it has been parked twice
//! is that its cost was only ever seen through a whole solve, where the search's own
//! noise sits on top of it. This isolates it: presolve is run twice on the same model,
//! once with probing cut off at its first candidate and once let run, and the
//! difference between the two is probing and nothing else.
use ripsolve::Problem;
use ripsolve::presolve::{Outcome, presolve_until};
use std::time::{Duration, Instant};

fn fixed_columns(problem: &Problem) -> usize {
    (0..problem.n_cols())
        .filter(|&j| problem.col_lb[j] >= problem.col_ub[j] - 1e-9)
        .count()
}

fn run(problem: &Problem, probe: bool) -> (Duration, usize, bool) {
    let mut reduced = problem.clone();
    // A deadline already past stops probing at its first candidate, which leaves the
    // cheap reductions and nothing else.
    let deadline = (!probe).then(|| Instant::now() - Duration::from_secs(1));
    let started = Instant::now();
    let outcome = presolve_until(&mut reduced, 20, deadline);
    let elapsed = started.elapsed();
    (
        elapsed,
        fixed_columns(&reduced),
        outcome == Outcome::Infeasible,
    )
}

fn main() {
    println!(
        "{:<16} {:>7} {:>9} {:>9} {:>9} {:>9}",
        "model", "cols", "cheap s", "probe s", "cheap fix", "probe fix"
    );
    for path in std::env::args().skip(1) {
        let problem = match Problem::from_file(std::path::Path::new(&path)) {
            Ok(p) => p,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let name = std::path::Path::new(&path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let (cheap_time, cheap_fixed, _) = run(&problem, false);
        let (probe_time, probe_fixed, infeasible) = run(&problem, true);
        let note = if infeasible { "  infeasible" } else { "" };
        println!(
            "{name:<16} {:>7} {:>9.2} {:>9.2} {cheap_fixed:>9} {probe_fixed:>9}{note}",
            problem.n_cols(),
            cheap_time.as_secs_f64(),
            probe_time.as_secs_f64(),
        );
    }
}
