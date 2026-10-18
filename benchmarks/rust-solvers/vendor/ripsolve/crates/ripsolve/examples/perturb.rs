//! Does bound perturbation break the degeneracy that stalls some relaxations?
//!
//! Widening every bound by a small, per-variable random amount makes a relaxation of
//! the model, so it cannot turn a feasible LP infeasible, and it moves the basic
//! variables off the bounds they are sitting exactly on, which is what makes a
//! degenerate step have length zero.
use ripsolve::Problem;
use ripsolve::lp::Lp;
use std::time::Instant;

/// SplitMix64, so the perturbation is reproducible.
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
    let mut p = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let mut rng = Rng(0x5DEECE66D);

    if scale > 0.0 {
        let widen = |lo: &mut f64, hi: &mut f64, rng: &mut Rng| {
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
        for i in 0..p.n_rows() {
            let (mut lo, mut hi) = (p.row_lb[i], p.row_ub[i]);
            widen(&mut lo, &mut hi, &mut rng);
            p.row_lb[i] = lo;
            p.row_ub[i] = hi;
        }
    }

    let start = Instant::now();
    let solved = Lp::relaxation(&p).solve_with_limit(500_000);
    println!(
        "scale {scale:e}: {:?} objective {:.6} in {} iterations, {:.1}s",
        solved.status,
        p.objective_value(solved.objective),
        solved.iterations,
        start.elapsed().as_secs_f64()
    );
}
