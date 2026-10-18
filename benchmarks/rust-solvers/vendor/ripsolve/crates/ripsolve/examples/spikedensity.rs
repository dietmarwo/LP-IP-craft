//! How sparse the entering column's solve `B^-1 a_q` actually comes out.
//!
//! This is the measurement that decides whether a hyper-sparse triangular solve would
//! pay. It was implemented once and reverted on the grounds that the solve runs 12 to
//! 33% dense, and that was measured on column-rich models; the group it would serve is
//! row-heavy -- `bley_xl1` is 17039 rows against 751 structural columns, so its basis is
//! nearly all logicals, which is the shape hyper-sparsity is about.
//!
//! A second attempt was wasted by counting every FTRAN, including the dense `B^-1 b`
//! recomputed at each refactorization. That reads ~95% on every model and decides
//! nothing. Only the two entering-column solves are counted here.
//!
//! One line per model: rows, columns, iterations, the mean density of `B^-1 a_q`, and
//! the share of solves at or under the 10% threshold a hyper-sparse solve needs to pay.
use ripsolve::Problem;
use ripsolve::lp::{HYPERSPARSE_SHARE, Lp, Tolerances};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cap: usize = args
        .iter()
        .find_map(|a| a.strip_prefix("--iterations="))
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000);

    println!(
        "{:<22} {:>7} {:>7} {:>9} {:>8} {:>9}  {}",
        "model", "rows", "cols", "iters", "density", "<=10%", "status"
    );
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let name = std::path::Path::new(path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let mut problem = Problem::from_file(std::path::Path::new(path)).unwrap();
        ripsolve::presolve::presolve(&mut problem, 20);
        let rows = problem.n_rows();
        let cols = problem.n_cols();
        let mut lp = Lp::relaxation(&problem).with_tolerances(Tolerances {
            entering_density_stats: true,
            ..Default::default()
        });
        let solved = lp.solve_with_limit(cap);

        // Density is per solve against the row count, which is the length of `alpha`.
        let solves = solved.entering_solves.max(1);
        let density = 100.0 * solved.entering_nonzeros as f64 / (solves as f64 * rows.max(1) as f64);
        let sparse = 100.0 * solved.entering_sparse_solves as f64 / solves as f64;
        println!(
            "{name:<22} {rows:>7} {cols:>7} {:>9} {density:>7.1}% {sparse:>8.1}%  {:?}",
            solved.iterations, solved.status,
        );
    }
    println!("\nthreshold for a hyper-sparse solve to pay: {:.0}%", 100.0 * HYPERSPARSE_SHARE);
}
