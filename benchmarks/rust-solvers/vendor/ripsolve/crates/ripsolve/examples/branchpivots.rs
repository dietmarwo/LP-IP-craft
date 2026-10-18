//! Pivots a child LP takes from its parent's basis, with no search around it.
//!
//! `branchpivots <model> [columns]`: solve the root, then fix each of the first N
//! fractional columns down and up in turn, warm starting from the root basis every time.
//! A tree search's node LPs are exactly these re-solves, so this is the per-node cost
//! with every other part of the search taken away.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let problem = Problem::from_file(std::path::Path::new(&args[1])).unwrap();
    let count: usize = args.get(2).map_or(20, |s| s.parse().unwrap());
    let mut lp = Lp::relaxation(&problem);
    let started = std::time::Instant::now();
    let root = lp.solve();
    assert_eq!(root.status, LpStatus::Optimal);
    println!(
        "root {:.6} in {} iterations, {:.2}s",
        root.objective,
        root.iterations,
        started.elapsed().as_secs_f64()
    );
    let fractional: Vec<usize> = (0..problem.n_cols())
        .filter(|&j| problem.is_integer(j) && (root.x[j] - root.x[j].round()).abs() > 1e-6)
        .take(count)
        .collect();
    let (mut total, mut solves) = (0usize, 0usize);
    let started = std::time::Instant::now();
    for &j in &fractional {
        let (lo, hi) = (problem.col_lb[j], problem.col_ub[j]);
        for (branch_lo, branch_hi) in [(lo, root.x[j].floor()), (root.x[j].ceil(), hi)] {
            lp.set_column_bounds(j, branch_lo, branch_hi);
            let child = lp.solve_warm(&root.basis, None, 1_000_000);
            total += child.iterations;
            solves += 1;
            println!(
                "  x{j} in [{branch_lo}, {branch_hi}]: {:?} {:.6} in {} iterations",
                child.status, child.objective, child.iterations
            );
        }
        lp.set_column_bounds(j, lo, hi);
    }
    println!(
        "{solves} children, {:.1} iterations each, {:.1} ms each",
        total as f64 / solves.max(1) as f64,
        started.elapsed().as_secs_f64() * 1000.0 / solves.max(1) as f64
    );
}
