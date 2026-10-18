//! What a relaxation's simplex iterations actually are, and what rule produced them.
//!
//! One line per model: seconds, iterations, the share that ran under Bland's rule, and
//! the share whose step moved nothing. Those two together are what a degenerate stall
//! looks like from outside, and neither is visible in an iteration count -- `cod105`'s
//! relaxation was 90% Bland and 94% zero-step and read as "slow" for three sessions.
//!
//! `--no-devex` prices by the largest reduced cost instead, which is what this did
//! before, so the two rules can be compared on the same models in seconds.
use ripsolve::Problem;
use ripsolve::lp::{Lp, Tolerances};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cap: usize = args
        .iter()
        .find_map(|a| a.strip_prefix("--iterations="))
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000);
    let devex = !args.iter().any(|a| a == "--no-devex");

    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let name = std::path::Path::new(path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let mut problem = Problem::from_file(std::path::Path::new(path)).unwrap();
        ripsolve::presolve::presolve(&mut problem, 20);
        let mut lp = Lp::relaxation(&problem).with_tolerances(Tolerances {
            primal_devex: devex,
            ..Default::default()
        });
        let started = std::time::Instant::now();
        let solved = lp.solve_with_limit(cap);
        let elapsed = started.elapsed().as_secs_f64();
        let share = |n: usize| 100.0 * n as f64 / solved.iterations.max(1) as f64;
        println!(
            "{name:<22} {elapsed:>7.1}s {:>8} iters  bland {:>5.1}%  degenerate {:>5.1}%  {:?}",
            solved.iterations,
            share(solved.bland_iterations),
            share(solved.degenerate_iterations),
            solved.status,
        );
    }
}
