//! How much more presolve finds if it is allowed to keep going.
use ripsolve::Problem;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    for rounds in [1usize, 5, 20, 100, 500] {
        let mut p = Problem::from_file(std::path::Path::new(&path)).unwrap();
        let before = (p.n_rows(), p.n_cols());
        let started = std::time::Instant::now();
        let out = ripsolve::presolve::presolve(&mut p, rounds);
        let stats = match out {
            ripsolve::presolve::Outcome::Reduced(s) => s,
            other => {
                println!("  rounds {rounds}: {other:?}");
                continue;
            }
        };
        println!(
            "  rounds {rounds:>4}: ran {:>3} | fixed {:>6} cols, removed {:>6} rows | {} x {} -> live {} x {} | {:.2}s",
            stats.rounds,
            stats.fixed_columns,
            stats.redundant_rows,
            before.0,
            before.1,
            p.n_rows(),
            p.n_cols(),
            started.elapsed().as_secs_f64()
        );
    }
}
