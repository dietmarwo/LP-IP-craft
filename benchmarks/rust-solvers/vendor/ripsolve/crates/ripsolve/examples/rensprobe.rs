//! Would fixing the relaxation's integral columns leave a model worth searching?
//!
//! The three instances waiting only on a good point all hold an exact or near-exact
//! bound, which means their relaxation is telling the truth about where the optimum is.
//! RENS takes that literally: fix every integer column the relaxation already puts on an
//! integer, and search what is left. This measures the two things that decide whether it
//! is worth building -- how much of the model it fixes, and what the remainder is worth
//! -- without putting it in the solver first.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};
use ripsolve::search::{self, Options};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let seconds: f64 = args.next().map_or(20.0, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    ripsolve::presolve::presolve(&mut problem, 20);

    let root = Lp::relaxation(&problem).solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("{name:<22} root relaxation did not finish");
        return;
    }

    let mut narrowed = problem.clone();
    let (mut fixed, mut integers) = (0usize, 0usize);
    for j in problem.integer_columns() {
        integers += 1;
        let value = root.x[j].round();
        if (root.x[j] - value).abs() <= 1e-6 {
            narrowed.col_lb[j] = value;
            narrowed.col_ub[j] = value;
            fixed += 1;
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
        "{name:<22} root {:>14.4}  fixed {fixed} of {integers} ({:.0}%)  sub-MIP {:?} {:?}",
        root.objective,
        100.0 * fixed as f64 / integers.max(1) as f64,
        found.status,
        found.objective,
    );
}
