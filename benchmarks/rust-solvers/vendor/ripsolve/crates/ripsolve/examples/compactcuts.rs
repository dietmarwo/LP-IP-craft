//! Does a compacted model yield cuts the uncompacted one does not?
//!
//! A reference solver restarts by physically shrinking the model -- `neos-820879` from
//! 9522 columns to 2476 -- where this solver keeps every fixed column in the matrix. Its
//! cut families are exhausted on the uncompacted model at a bound well short of what is
//! needed, and the open question is whether that is a property of the model or of its
//! *representation*. This builds both and separates from each.
use ripsolve::Problem;
use ripsolve::cuts::{self, Cut};
use ripsolve::lp::{Lp, LpStatus};

fn re_solve(base: &Problem, cuts: &[Cut]) -> Option<f64> {
    let mut with = base.clone();
    with.add_cuts(cuts);
    let solved = Lp::relaxation(&with).solve_with_limit(1_000_000);
    (solved.status == LpStatus::Optimal).then_some(solved.objective)
}

fn separate(problem: &Problem, label: &str, limit: usize) {
    let mut lp = Lp::relaxation(problem);
    let root = lp.solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("  {label:<14} root relaxation did not finish");
        return;
    }
    let conflicts = cuts::Conflicts::of(problem);
    let mut all: Vec<Cut> = Vec::new();
    all.extend(cuts::separate(problem, &root.x, limit));
    all.extend(cuts::separate_mir(problem, &root.x, limit));
    all.extend(cuts::separate_cliques(problem, &conflicts, &root.x, limit));
    all.extend(cuts::separate_gomory(&lp, &root.basis, &root.x, limit));
    all.extend(cuts::separate_mod2(problem, &root.x, limit));
    let after = re_solve(problem, &all);
    println!(
        "  {label:<14} {} cols, {} rows, root {:.4}, {} cuts, bound {}",
        problem.n_cols(),
        problem.n_rows(),
        problem.objective_value(root.objective),
        all.len(),
        match after {
            Some(v) => format!("{:.4}", problem.objective_value(v)),
            None => "did not solve".to_string(),
        }
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let limit: usize = args.next().map_or(300, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    ripsolve::presolve::presolve(&mut problem, 20);

    // The state the solver is actually in when it would restart: the root solved, its
    // reduced costs read against the incumbent, and the columns that follow pinned.
    let mut lp = Lp::relaxation(&problem);
    let root = lp.solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("{name}: root relaxation did not finish");
        return;
    }
    let Some(costs) = lp.reduced_costs(&root.basis) else {
        println!("{name}: no reduced costs");
        return;
    };
    // The incumbent to fix against, read from a solution file so the comparison is made
    // at the point a restart would actually happen rather than at a guess.
    let incumbent: f64 = args
        .next()
        .map(|p| {
            let by_name: std::collections::HashMap<String, f64> = std::fs::read_to_string(p)
                .unwrap()
                .lines()
                .filter_map(|l| {
                    let mut it = l.split_whitespace();
                    Some((it.next()?.to_string(), it.next()?.parse().ok()?))
                })
                .collect();
            problem
                .obj
                .iter()
                .enumerate()
                .map(|(j, c)| c * by_name[&problem.col_names[j]])
                .sum()
        })
        .expect("solution file");
    let room = incumbent - root.objective;
    let mut fixed = problem.clone();
    for (j, entry) in costs.iter().enumerate() {
        let Some((d, at_upper)) = *entry else {
            continue;
        };
        let (lo, hi) = (fixed.col_lb[j], fixed.col_ub[j]);
        if lo >= hi || d.abs() <= 1e-9 {
            continue;
        }
        if (room / d.abs()).mul_add(1.0 + 1e-9, 1e-9) < 1.0 {
            if at_upper {
                fixed.col_lb[j] = hi;
            } else {
                fixed.col_ub[j] = lo;
            }
        }
    }
    let pinned = (0..fixed.n_cols())
        .filter(|&j| fixed.col_lb[j] >= fixed.col_ub[j])
        .count();
    println!(
        "{name}: incumbent {incumbent}, {pinned} of {} columns pinned",
        fixed.n_cols()
    );
    separate(&fixed, "uncompacted", limit);

    // The sequence a restart actually performs: cut, fix against the incumbent using the
    // cut model's own reduced costs, compact, and start again. Each round should hand
    // the next a smaller model whose relaxation is worth more.
    let mut current = fixed;
    let mut offset_shift = 0.0f64;
    for round in 0..6 {
        let Ok(Some((smaller, _))) = ripsolve::compact::compact(&current, 1e-9) else {
            println!("  round {round}: nothing to compact");
            break;
        };
        let mut lp = Lp::relaxation(&smaller);
        let root = lp.solve_with_limit(1_000_000);
        if root.status != LpStatus::Optimal {
            println!("  round {round}: root did not finish");
            break;
        }
        let conflicts = cuts::Conflicts::of(&smaller);
        let mut all: Vec<Cut> = Vec::new();
        all.extend(cuts::separate(&smaller, &root.x, limit));
        all.extend(cuts::separate_mir(&smaller, &root.x, limit));
        all.extend(cuts::separate_cliques(&smaller, &conflicts, &root.x, limit));
        all.extend(cuts::separate_gomory(&lp, &root.basis, &root.x, limit));
        all.extend(cuts::separate_mod2(&smaller, &root.x, limit));
        let mut cut_model = smaller.clone();
        cut_model.add_cuts(&all);
        let mut cut_lp = Lp::relaxation(&cut_model);
        let after = cut_lp.solve_with_limit(1_000_000);
        if after.status != LpStatus::Optimal {
            println!("  round {round}: cut relaxation did not finish");
            break;
        }
        let _ = offset_shift;
        offset_shift = 0.0;
        println!(
            "  round {round}: {} cols, {} cuts, root {:.4} -> {:.4}",
            smaller.n_cols(),
            all.len(),
            cut_model.objective_value(root.objective),
            cut_model.objective_value(after.objective)
        );
        // Fix again on the cut model's reduced costs, then loop.
        let Some(costs) = cut_lp.reduced_costs(&after.basis) else {
            break;
        };
        let room = incumbent - after.objective;
        let mut next = cut_model.clone();
        let mut newly = 0usize;
        for (j, entry) in costs.iter().enumerate() {
            let Some((d, at_upper)) = *entry else {
                continue;
            };
            let (lo, hi) = (next.col_lb[j], next.col_ub[j]);
            if lo >= hi || d.abs() <= 1e-9 {
                continue;
            }
            if (room / d.abs()).mul_add(1.0 + 1e-9, 1e-9) < 1.0 {
                if at_upper {
                    next.col_lb[j] = hi
                } else {
                    next.col_ub[j] = lo
                }
                newly += 1;
            }
        }
        if newly == 0 {
            println!("  round {round}: nothing further fixed, stopping");
            break;
        }
        current = next;
    }
}
