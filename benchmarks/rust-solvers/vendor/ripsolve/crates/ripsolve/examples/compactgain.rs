//! What compacting a presolved model is worth to the search that follows it.
//!
//! `compact` is built and tested and nothing calls it, because the general version was
//! measured as "correct, and 14% slower for nothing" on models presolve barely touches.
//! A model probing has decided nearly all of is the opposite case: `ex10` leaves 408 of
//! 17680 columns free and still carries every one of its 69608 rows into the basis. This
//! presolves once and then solves the same model twice, compacted and not.
use ripsolve::Problem;
use ripsolve::search::{self, Options};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let limit: f64 = args.next().map_or(600.0, |v| v.parse().unwrap());

    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let started = Instant::now();
    ripsolve::presolve::presolve(&mut problem, 20);
    println!(
        "presolved in {:.1}s to {} rows, {} cols, {} free",
        started.elapsed().as_secs_f64(),
        problem.n_rows(),
        problem.n_cols(),
        (0..problem.n_cols())
            .filter(|&j| problem.col_lb[j] < problem.col_ub[j])
            .count()
    );

    let options = || Options {
        time_limit: Some(Duration::from_secs_f64(limit)),
        // Presolve has already run, and running it again inside the search would time
        // the probing twice and hide the thing being measured.
        presolve: false,
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
        ..Options::default()
    };

    let compacted = ripsolve::compact::compact(&problem, 1e-9).expect("not infeasible");
    match &compacted {
        Some((small, _)) => println!(
            "compacted to {} rows, {} cols, {} nonzeros",
            small.n_rows(),
            small.n_cols(),
            small.matrix.nnz()
        ),
        None => println!("nothing to compact"),
    }

    for (label, model) in [
        ("compacted", compacted.as_ref().map(|(p, _)| p)),
        ("as presolved", Some(&problem)),
    ] {
        let Some(model) = model else { continue };
        let started = Instant::now();
        let solution = search::solve(model, options());
        println!(
            "{label:<14} {:?} in {:.1}s, {} nodes, objective {:?}",
            solution.status,
            started.elapsed().as_secs_f64(),
            solution.nodes,
            solution.objective
        );
    }
}
