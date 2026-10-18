//! Does adding the dive's clauses to the LP move the root bound?
//!
//! The dive learns clauses and propagation obeys them, but propagation does not change
//! what the relaxation says. A reference solver adds them as *rows*: on
//! `chromaticindex32-8` its constraint count goes 2125 to 2326 and the root bound goes 3
//! to 4 in ten further simplex iterations. This checks the same thing here before any of
//! it is built into the search.
use ripsolve::cuts::{Conflicts, Cut};
use ripsolve::lp::{Lp, LpStatus};
use ripsolve::{Problem, propagate};

fn main() {
    let path = std::env::args().nth(1).expect("model path");
    let keep: usize = std::env::args().nth(2).map_or(500, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    ripsolve::presolve::presolve(&mut problem, 20);

    let root = Lp::relaxation(&problem).solve_with_limit(1_000_000);
    println!("root relaxation {:?} {:.6}", root.status, root.objective);

    // Dive against "strictly better than the known optimum", which is what makes the
    // model contradict and therefore what produces clauses at all.
    let cutoff: f64 = std::env::args()
        .nth(3)
        .map_or(3.0, |v| v.parse().unwrap());
    let objective: Vec<(usize, f64)> = problem
        .obj
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c != 0.0)
        .map(|(j, &c)| (j, c))
        .collect();
    let mut bounded = problem.clone();
    bounded.add_cuts(&[Cut { coefficients: objective, lb: f64::NEG_INFINITY, ub: cutoff }]);

    let rows = bounded.matrix.to_csr();
    let conflicts = Conflicts::of(&bounded);
    let mut scratch = propagate::Scratch::new(bounded.n_cols());
    let started = std::time::Instant::now();
    let learned = propagate::rapid_learn(
        &bounded, &rows, &conflicts, &mut scratch, 50_000, 64, 50_000, 50_000, None, 1e-9,
    );
    println!(
        "dive: {} decisions, {} clauses in {:.2}s",
        learned.decisions,
        learned.clauses.len(),
        started.elapsed().as_secs_f64()
    );

    {
        let mut lens: Vec<usize> = learned.clauses.iter().map(Vec::len).collect();
        lens.sort_unstable();
        if !lens.is_empty() {
            println!(
                "clause length: min {} median {} max {}  (a reference solver's average 4.1)",
                lens[0], lens[lens.len() / 2], lens[lens.len() - 1]
            );
        }
    }
    // Shortest first: a short clause cuts more off the relaxation and costs less to carry.
    let mut clauses = learned.clauses;
    clauses.sort_by_key(Vec::len);
    clauses.truncate(keep);
    // "at least one of these literals holds" as a row: sum of x_j over the true literals
    // plus sum of (1 - x_j) over the false ones is at least one.
    let rows: Vec<Cut> = clauses
        .iter()
        .map(|clause| {
            let mut lb = 1.0;
            let coefficients: Vec<(usize, f64)> = clause
                .iter()
                .map(|&(j, want)| {
                    if want {
                        (j as usize, 1.0)
                    } else {
                        lb -= 1.0;
                        (j as usize, -1.0)
                    }
                })
                .collect();
            Cut { coefficients, lb, ub: f64::INFINITY }
        })
        .collect();

    let mut with = problem.clone();
    with.add_cuts(&rows);
    let after = Lp::relaxation(&with).solve_with_limit(1_000_000);
    println!(
        "with {} clause rows: {:?} {:.6}  ({} iterations)",
        rows.len(),
        after.status,
        after.objective,
        after.iterations
    );
    if after.status == LpStatus::Optimal && after.objective > root.objective + 1e-6 {
        println!("THE BOUND MOVED: {:.6} -> {:.6}", root.objective, after.objective);
    } else {
        println!("the bound did not move");
    }
}
