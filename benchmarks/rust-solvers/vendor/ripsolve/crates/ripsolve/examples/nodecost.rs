//! Where a search's simplex iterations go: with the in-tree heuristics and without.
//!
//! `nodecost <model> [seconds] [threads]`. The iterations per node a reference solver
//! spends are its node LPs; ours also count every dive a heuristic takes, so the two
//! numbers are only comparable once the heuristics are taken out.
use ripsolve::Problem;
use ripsolve::search::{Options, solve};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let problem = Problem::from_file(std::path::Path::new(&args[1])).unwrap();
    let seconds: f64 = args.get(2).map_or(30.0, |s| s.parse().unwrap());
    let threads: usize = args.get(3).map_or(1, |s| s.parse().unwrap());
    for (label, heuristics) in [("heuristics on ", true), ("heuristics off", false)] {
        let mut options = Options {
            time_limit: Some(Duration::from_secs_f64(seconds)),
            threads,
            ..Options::default()
        };
        if !heuristics {
            options.heuristic_frequency = 0;
            options.improvement_frequency = 0;
        }
        let started = std::time::Instant::now();
        let s = solve(&problem, options);
        println!(
            "{label}: {:?} bound {} obj {:?} | {} nodes, {} iterations, {:.1} per node, {:.1} nodes/s",
            s.status,
            s.bound,
            s.objective,
            s.nodes,
            s.simplex_iterations,
            s.simplex_iterations as f64 / s.nodes.max(1) as f64,
            s.nodes as f64 / started.elapsed().as_secs_f64()
        );
    }
}
