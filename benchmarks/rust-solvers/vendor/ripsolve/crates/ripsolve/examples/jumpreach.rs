//! How long the LP-free feasibility search needs, given the room to finish.
//!
//! It is asked for a share of the run and only after the relaxation has failed, so on a
//! model whose relaxation eats the budget it is asked for what is left of nothing. This
//! asks it directly, with a budget of its own, to find out whether the models it is
//! meant for are reachable at all.
use ripsolve::Problem;
use ripsolve::heuristic::{self, Limits};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let seconds: f64 = args.next().map_or(30.0, |v| v.parse().unwrap());
    let moves: usize = args.next().map_or(50_000_000, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    ripsolve::presolve::presolve(&mut problem, 20);

    let tries: usize = args.next().map_or(1, |v| v.parse().unwrap());
    let began = Instant::now();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for attempt in 0..tries {
        // The first start is the one the solver uses; the rest are random, to find out
        // whether where it begins is what decides whether it arrives.
        let start: Vec<f64> = (0..problem.n_cols())
            .map(|j| {
                if attempt == 0 {
                    problem.col_lb[j].max(0.0)
                } else {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    if state & 1 == 0 {
                        problem.col_lb[j]
                    } else {
                        problem.col_ub[j]
                    }
                }
            })
            .collect();
        let found = heuristic::feasibility_jump(
            &problem,
            &start,
            &Limits::default(),
            moves,
            Some(began + Duration::from_secs_f64(seconds)),
        );
        if let Some(point) = found {
            println!(
                "{name:<24} feasible at objective {} on try {attempt} in {:.2}s",
                point.objective,
                began.elapsed().as_secs_f64()
            );
            return;
        }
        if began.elapsed().as_secs_f64() >= seconds {
            break;
        }
    }
    println!(
        "{name:<24} nothing in {tries} tries, {:.2}s",
        began.elapsed().as_secs_f64()
    );
}
