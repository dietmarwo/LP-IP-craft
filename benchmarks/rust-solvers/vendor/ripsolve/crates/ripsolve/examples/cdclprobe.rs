//! Run the conflict-driven search on a model and say what it found.
//!
//! `cdclprobe <model> [cutoff] [conflicts] [seconds]`. The model is presolved first, as
//! the search would see it. With a cutoff the search looks only for points whose
//! objective is at most that, which is how the root would aim it once it has an
//! incumbent; without one it is looking for any point at all.
use ripsolve::Problem;
use ripsolve::cdcl::{Limits, Verdict, search};
use ripsolve::cuts::Conflicts;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut problem = Problem::from_file(std::path::Path::new(&args[1])).unwrap();
    let cutoff: Option<f64> = args.get(2).and_then(|s| s.parse().ok());
    let max_conflicts: usize = args.get(3).map_or(100_000, |s| s.parse().unwrap());
    let seconds: f64 = args.get(4).map_or(60.0, |s| s.parse().unwrap());

    let started = Instant::now();
    if let Some((smaller, _)) = ripsolve::aggregate::aggregate(&problem, 1e-9).unwrap() {
        problem = smaller;
    }
    ripsolve::presolve::presolve(&mut problem, 20);
    let conflicts = Conflicts::of(&problem);
    println!("presolved in {:.2}s", started.elapsed().as_secs_f64());

    if args.get(2).map(String::as_str) == Some("opt") {
        let started = Instant::now();
        let out = ripsolve::cdcl::optimize(
            &problem,
            &conflicts,
            None,
            |v| v - 1.0,
            Limits {
                max_conflicts,
                deadline: Some(started + Duration::from_secs_f64(seconds)),
                export_length: 32,
                export_count: 100_000,
                stall_conflicts: usize::MAX,
                stall_seconds: f64::INFINITY,
                cancel: None,
            },
            |_, _| {},
        );
        let events: Vec<String> = out
            .trace
            .iter()
            .map(|(c, t)| format!("{c}@{t:.2}s"))
            .collect();
        println!("events: {}", events.join(" "));
        println!(
            "optimize: best {:?} proved {} after {} improvements, {} conflicts, {:.2}s",
            out.best.as_ref().map(|(v, _)| problem.objective_value(*v)),
            out.proved,
            out.improvements,
            out.conflicts,
            started.elapsed().as_secs_f64()
        );
        return;
    }
    // The objective as the solver holds it; the offset moves with it.
    let row = cutoff.map(|c| {
        let entries: Vec<(usize, f64)> = problem
            .obj
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, v)| v != 0.0)
            .collect();
        (entries, c - problem.obj_offset)
    });
    let started = Instant::now();
    let out = search(
        &problem,
        &conflicts,
        row.as_ref(),
        Limits {
            max_conflicts,
            deadline: Some(started + Duration::from_secs_f64(seconds)),
            export_length: 32,
            export_count: 100_000,
            stall_conflicts: usize::MAX,
                stall_seconds: f64::INFINITY,
                cancel: None,
        },
    );
    let lengths: Vec<usize> = out.clauses.iter().map(Vec::len).collect();
    let mean = lengths.iter().sum::<usize>() as f64 / lengths.len().max(1) as f64;
    println!(
        "{} in {:.2}s: {} conflicts, {} decisions, {} clauses (mean length {:.1}), {} fixings",
        match &out.verdict {
            Verdict::Solution(x) => format!("solution, objective {}", problem.objective_value(
                problem.obj.iter().zip(x).map(|(c, v)| c * v).sum::<f64>() + problem.obj_offset)),
            Verdict::Infeasible => "infeasible".to_string(),
            Verdict::Unknown => "unknown".to_string(),
        },
        started.elapsed().as_secs_f64(),
        out.conflicts,
        out.decisions,
        out.clauses.len(),
        mean,
        out.fixings.len()
    );
}
