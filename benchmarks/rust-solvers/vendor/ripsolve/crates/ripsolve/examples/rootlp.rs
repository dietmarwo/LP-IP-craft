//! How long the root relaxation takes, on its own, and whether it finishes at all.
//!
//! Every diagnostic here so far reads the relaxation through a whole solve, where the
//! node count is the only visible symptom and "one node in a minute" could be the LP,
//! the cut loop or the heuristics. This runs presolve and then the relaxation and
//! nothing else, so a model that never gets past its first node says which.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let cap: usize = std::env::var("ROOTLP_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000_000);
    println!(
        "{:<22} {:>9} {:>8} {:>9} {:>7} {:>9} {:>14}",
        "model", "rows", "cols", "nnz", "pre s", "lp s", "status"
    );
    for path in args.by_ref() {
        let name = std::path::Path::new(&path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
        let started = Instant::now();
        ripsolve::presolve::presolve(&mut problem, 20);
        let presolved = started.elapsed().as_secs_f64();

        let dual = std::env::var_os("ROOTLP_DUAL").is_some();
        let started = Instant::now();
        let mut lp = Lp::relaxation(&problem);
        let solved = if dual {
            lp.solve_cold_dual(cap)
        } else {
            lp.solve_with_limit(cap)
        };
        let elapsed = started.elapsed().as_secs_f64();
        let status = match solved.status {
            LpStatus::Optimal => format!("optimal {:.4}", solved.objective),
            other => format!("{other:?}"),
        };
        println!(
            "{name:<22} {:>9} {:>8} {:>9} {presolved:>7.2} {elapsed:>9.2} {status:>14}  iters {} ({:.0}/s)",
            problem.n_rows(),
            problem.n_cols(),
            problem.matrix.nnz(),
            solved.iterations,
            solved.iterations as f64 / elapsed.max(1e-9),
        );
    }
}
