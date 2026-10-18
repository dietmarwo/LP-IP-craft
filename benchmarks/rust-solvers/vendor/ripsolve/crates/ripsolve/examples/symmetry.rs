//! Report the column symmetry of each model given, and what it cost to find.
//!
//! One line per model: size, the seconds spent, how much of that was building the graph
//! rather than searching it, the graph's size, the refinement work done, and then what
//! came back -- generators, columns moved, orbits, and the largest orbit, which is the
//! number that decides whether the search can use any of it.
//!
//! `--seconds=` and `--stall-work=` set the budget, so the cost of a setting can be read
//! off the whole set in minutes rather than by running a benchmark on top of a solve.
use ripsolve::Problem;
use ripsolve::symmetry::{Budget, detect};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .find_map(|a| a.strip_prefix(name).map(str::to_string))
    };
    let default = Budget::default();
    let budget = Budget {
        time: flag("--seconds=")
            .and_then(|s| s.parse().ok())
            .map(Duration::from_secs_f64)
            .or(default.time),
        stall_work: flag("--stall-work=")
            .and_then(|s| s.parse().ok())
            .unwrap_or(default.stall_work),
        max_vertices: flag("--max-vertices=")
            .and_then(|s| s.parse().ok())
            .unwrap_or(default.max_vertices),
        max_work: flag("--max-work=")
            .and_then(|s| s.parse().ok())
            .unwrap_or(default.max_work),
    };

    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let name = std::path::Path::new(path)
            .file_stem()
            .map_or_else(|| path.clone(), |s| s.to_string_lossy().into_owned());
        let Ok(problem) = Problem::from_file(std::path::Path::new(path)) else {
            println!("{name}\tunreadable");
            continue;
        };

        let started = Instant::now();
        let symmetry = detect(&problem, budget);
        let elapsed = started.elapsed().as_secs_f64();

        let orbits = symmetry.nontrivial_orbits();
        let largest = orbits.iter().map(Vec::len).max().unwrap_or(0);
        println!(
            "{name}\t{}x{}\t{elapsed:.2}s\tbuild {:.2}s\tverts {}\twork {}\tgens {}\tmoved {}/{}\torbits {}\tlargest {largest}\tnodes {}{}",
            problem.n_rows(),
            problem.n_cols(),
            symmetry.build_seconds,
            symmetry.vertices,
            symmetry.work,
            symmetry.generators.len(),
            symmetry.moved,
            problem.n_cols(),
            orbits.len(),
            symmetry.nodes,
            if symmetry.exhausted_budget {
                "\tBUDGET"
            } else {
                ""
            },
        );
    }
}
