//! Would restarting on the narrowed model pay, before anything is built to do it?
//!
//! Reduced cost fixing collects bounds as the incumbent improves, and on `n2seq36f`
//! ends the minute with 89% of the columns decided. The search walks that model but
//! never re-derives anything from it: the bound it is trying to beat still comes from
//! the relaxation of the *original* root, and on `n2seq36f` it never moves off it.
//!
//! A restart would presolve and re-cut the narrowed model instead. This simulates one,
//! by solving for a first budget, applying the fixing the incumbent has earned, and
//! solving what is left for a second, so the answer is known before the search is
//! restructured to allow it.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};
use ripsolve::search::{self, Options};
use std::time::Duration;

fn internal_objective(problem: &Problem, x: &[f64]) -> f64 {
    problem.obj.iter().zip(x).map(|(c, v)| c * v).sum()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let first: f64 = args.next().map_or(30.0, |v| v.parse().unwrap());
    let second: f64 = args.next().map_or(30.0, |v| v.parse().unwrap());
    let problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let options = |limit: f64| Options {
        time_limit: Some(Duration::from_secs_f64(limit)),
        ..Options::default()
    };

    let before = search::solve(&problem, options(first));
    let Some(reported) = before.objective else {
        println!("{name}: no incumbent in {first}s, nothing to restart from");
        return;
    };
    println!(
        "{name}: first {first}s -> {reported}, bound {}, gap {:.4}%, {} nodes",
        before.bound,
        before.gap() * 100.0,
        before.nodes
    );

    // The narrowed model the restart would see: presolve, then the root's own reduced
    // costs read against the incumbent the first pass earned.
    let incumbent = internal_objective(&problem, &before.x);
    let mut narrowed = problem.clone();
    ripsolve::presolve::presolve(&mut narrowed, 20);
    let mut lp = Lp::relaxation(&narrowed);
    let root = lp.solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("{name}: root relaxation did not finish, nothing to narrow with");
        return;
    }
    let room = incumbent - root.objective;
    let costs = lp.reduced_costs(&root.basis).expect("root is optimal");
    let mut fixed = 0usize;
    for (j, entry) in costs.iter().enumerate() {
        let Some((d, at_upper)) = *entry else {
            continue;
        };
        let (lo, hi) = (narrowed.col_lb[j], narrowed.col_ub[j]);
        if lo >= hi || !lo.is_finite() || !hi.is_finite() || d.abs() <= 1e-9 {
            continue;
        }
        let travel = (room / d.abs()).mul_add(1.0 + 1e-9, 1e-9);
        if travel >= hi - lo {
            continue;
        }
        let travel = if narrowed.is_integer(j) {
            (travel + 1e-6).floor()
        } else {
            travel
        };
        if at_upper {
            narrowed.col_lb[j] = (hi - travel).clamp(lo, hi);
        } else {
            narrowed.col_ub[j] = (lo + travel).clamp(lo, hi);
        }
        fixed += 1;
    }
    let decided = (0..narrowed.n_cols())
        .filter(|&j| narrowed.col_lb[j] >= narrowed.col_ub[j])
        .count();
    println!(
        "{name}: narrowed {fixed} columns, {decided} of {} now decided",
        narrowed.n_cols()
    );

    let after = search::solve(&narrowed, options(second));
    println!(
        "{name}: restarted {second}s -> {:?} {:?}, bound {}, gap {:.4}%, {} nodes",
        after.status,
        after.objective,
        after.bound,
        after.gap() * 100.0,
        after.nodes
    );
}
