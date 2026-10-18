//! Perturb, solve cold, restore the true bounds, and re-solve from that basis.
//!
//! Perturbing alone is not a solved relaxation: it solves a weaker problem whose bound
//! is valid and nearly worthless, `neos-1324574` reporting -0.0001 where the true
//! optimum is 4.5. The point of it is the basis, not the bound. A basis optimal for a
//! nearby problem is a legitimate warm start for this one, which is the thing a basis
//! stuck in this one's degeneracy is not.
//!
//! Two perturbations are compared. Widening the rows relaxes the constraints
//! themselves; widening only the columns leaves every constraint exact and still moves
//! the basic variables off the bounds they sit on, which is where the zero-length steps
//! come from.
use ripsolve::Problem;
use ripsolve::lp::{Lp, LpStatus};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let scale: f64 = args.next().unwrap_or_else(|| "1e-6".into()).parse().unwrap();
    let rows_too = args.next().unwrap_or_else(|| "cols".into()) == "rows";
    let limit: usize = 2_000_000;

    let original = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let mut p = original.clone();
    let mut rng = Rng(0x5DEECE66D);
    let mut widen = |lo: &mut f64, hi: &mut f64, rng: &mut Rng| {
        let reach = scale * (1.0 + lo.abs().max(hi.abs()).min(1e6)) * (1.0 + rng.next());
        if lo.is_finite() {
            *lo -= reach;
        }
        if hi.is_finite() {
            *hi += reach;
        }
    };
    for j in 0..p.n_cols() {
        let (mut lo, mut hi) = (p.col_lb[j], p.col_ub[j]);
        widen(&mut lo, &mut hi, &mut rng);
        p.col_lb[j] = lo;
        p.col_ub[j] = hi;
    }
    if rows_too {
        for i in 0..p.n_rows() {
            let (mut lo, mut hi) = (p.row_lb[i], p.row_ub[i]);
            widen(&mut lo, &mut hi, &mut rng);
            p.row_lb[i] = lo;
            p.row_ub[i] = hi;
        }
    }

    let started = Instant::now();
    let loose = Lp::relaxation(&p).solve_with_limit(limit);
    let after_first = started.elapsed().as_secs_f64();
    if loose.status != LpStatus::Optimal {
        println!("perturbed solve did not finish: {:?} after {after_first:.1}s", loose.status);
        return;
    }

    // The bound that matters comes from the true model, warm started from the basis the
    // perturbed one ended on.
    let exact = Lp::relaxation(&original);
    let cleaned = exact.solve_with_rows(&loose.basis, &[], None, limit);
    let total = started.elapsed().as_secs_f64();
    println!(
        "scale {scale:e} {} | perturbed {:.6} in {} it {:.1}s | cleaned {:?} {:.6} in {} it | total {:.1}s",
        if rows_too { "rows+cols" } else { "cols only" },
        original.objective_value(loose.objective),
        loose.iterations,
        after_first,
        cleaned.status,
        original.objective_value(cleaned.objective),
        cleaned.iterations,
        total
    );
}
