//! Is a good point inside the neighbourhood the root's reduced costs pick out?
//!
//! The neighbourhood search either finds one quickly or not at all, and those are
//! different problems: the first wants a bigger budget, the second wants a different
//! neighbourhood. This builds the same neighbourhood the solver builds and searches it
//! with as long as it takes, so the two can be told apart before either is attempted.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};
use ripsolve::search::{self, Options};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let seconds: f64 = args.next().map_or(60.0, |v| v.parse().unwrap());
    let target: f64 = args.next().map_or(0.5, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    ripsolve::presolve::presolve(&mut problem, 20);

    let mut lp = Lp::relaxation(&problem);
    let root = lp.solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("{name:<20} root relaxation did not finish");
        return;
    }
    let Some(costs) = lp.reduced_costs(&root.basis) else {
        println!("{name:<20} no reduced costs");
        return;
    };

    // The same construction the solver makes: entries ordered by the incumbent each
    // needs, applied until the target share of the model is decided.
    let mut entries: Vec<(f64, usize, f64)> = Vec::new();
    for (j, entry) in costs.iter().enumerate() {
        let Some((d, at_upper)) = *entry else {
            continue;
        };
        let (lo, hi) = (problem.col_lb[j], problem.col_ub[j]);
        if lo >= hi || !lo.is_finite() || !hi.is_finite() || !problem.is_integer(j) {
            continue;
        }
        if d.abs() <= 1e-6 {
            continue;
        }
        entries.push((root.objective + d.abs(), j, if at_upper { hi } else { lo }));
    }
    entries.sort_by(|a, b| b.0.total_cmp(&a.0));

    let integers = problem.integer_columns().count();
    let mut narrowed = problem.clone();
    let mut fixed = 0usize;
    for (_, j, value) in &entries {
        if narrowed.col_lb[*j] >= narrowed.col_ub[*j] {
            continue;
        }
        narrowed.col_lb[*j] = *value;
        narrowed.col_ub[*j] = *value;
        fixed += 1;
        if fixed as f64 >= target * integers as f64 {
            break;
        }
    }
    let found = search::solve(
        &narrowed,
        Options {
            time_limit: Some(Duration::from_secs_f64(seconds)),
            ..Options::default()
        },
    );
    println!(
        "{name:<20} fixed {fixed} of {integers} ({:.0}%)  neighbourhood {:?} {:?}",
        100.0 * fixed as f64 / integers.max(1) as f64,
        found.status,
        found.objective
    );
}
