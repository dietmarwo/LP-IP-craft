//! How much of the gap is the primal side, measured by removing it entirely.
//!
//! An instance short of a *point* and one short of a *bound* look the same from the
//! outside -- both time out with a gap -- and they want opposite work. Handing the
//! search a known optimum and asking whether it can then prove optimality separates
//! them: if it closes, everything missing was primal and a better heuristic converts it;
//! if it does not, the bound is short too and no heuristic will.
use ripsolve::Problem;
use ripsolve::search::{self, Options};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let solution =
        std::fs::read_to_string(args.next().expect("solution file: `name value` per line"))
            .unwrap();
    let seconds: f64 = args.next().map_or(60.0, |v| v.parse().unwrap());
    let problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    // Keyed by column name, not by position. Two readers of the same MPS file need not
    // agree on column order and these two do not: on `neos18` one starts at `r_0` and
    // the other at `x_1_0`, so a vector matched by position is a permutation of the
    // answer, scores 40 against a true 16, and violates 536 rows. It looks exactly like
    // an infeasible point, which is what it was mistaken for.
    let by_name: std::collections::HashMap<&str, f64> = solution
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            Some((it.next()?, it.next()?.parse().ok()?))
        })
        .collect();
    let missing = problem
        .col_names
        .iter()
        .filter(|n| !by_name.contains_key(n.as_str()))
        .count();
    if missing > 0 {
        println!(
            "{name:<20} {missing} of {} columns not named in the solution",
            problem.n_cols()
        );
        return;
    }
    let start: Vec<f64> = problem
        .col_names
        .iter()
        .map(|n| by_name[n.as_str()])
        .collect();

    let options = Options {
        time_limit: Some(Duration::from_secs_f64(seconds)),
        ..Options::default()
    };
    let feasible = ripsolve::heuristic::is_feasible(&problem, &start, 1e-6);
    let seeded = search::solve_from(&problem, options, Some(&start));
    if !feasible {
        println!("{name:<20} the reference point is not feasible for this model");
    }
    println!(
        "{name:<20} seeded with the optimum: {:?} {:?}, bound {:.4}, gap {:.4}%",
        seeded.status,
        seeded.objective,
        seeded.bound,
        seeded.gap() * 100.0
    );
}
