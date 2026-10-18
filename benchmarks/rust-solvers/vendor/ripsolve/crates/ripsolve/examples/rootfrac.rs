//! How many integer columns the root relaxation leaves fractional, and how fractional.
//!
//! A GMI cut needs a *basic* integer column with a fractional value, so this is the
//! supply the Gomory loop draws on. `chromaticindex32-8` produces one cut from 2111
//! rows and this says whether that is the generator being fussy or the relaxation
//! having nothing to offer it.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};

fn main() {
    for path in std::env::args().skip(1) {
        let name = std::path::Path::new(&path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
        ripsolve::presolve::presolve(&mut problem, 20);
        let mut lp = Lp::relaxation(&problem);
        let solved = lp.solve_with_limit(1_000_000);
        if solved.status != LpStatus::Optimal {
            println!("{name:<24} root relaxation did not finish ({:?})", solved.status);
            continue;
        }
        let integer: Vec<usize> = (0..problem.n_cols())
            .filter(|&j| problem.col_type[j] != ripsolve::model::VarType::Continuous)
            .collect();
        let frac = |v: f64| v - v.floor();
        let fractional: Vec<f64> = integer
            .iter()
            .map(|&j| frac(solved.x[j]))
            .filter(|f| *f > 1e-9 && *f < 1.0 - 1e-9)
            .collect();
        // The generator declines anything within 0.01 of an integer, so count those too.
        let usable = fractional.iter().filter(|f| **f >= 0.01 && **f <= 0.99).count();
        println!(
            "{name:<24} obj {:>14.4}  int cols {:>6}  fractional {:>6}  usable {:>6}",
            solved.objective, integer.len(), fractional.len(), usable
        );
    }
}
