//! Which cut family moves the bound, and by how much, on one model.
//!
//! The root cut loop reports one number: cuts added, and the bound before and after.
//! On `n2seq36f` that reads "1019 added, 52000 -> 52000", which says the loop is not
//! working and not which half of it. This runs each family on its own against the same
//! relaxation, adds what it finds, and re-solves, so the answer is per family and the
//! families cannot hide behind one another.
use ripsolve::Problem;
use ripsolve::cuts::{self, Cut};
use ripsolve::lp::{Lp, LpStatus};

fn re_solve(base: &Problem, cuts: &[Cut]) -> Option<f64> {
    let mut with = base.clone();
    with.add_cuts(cuts);
    let solved = Lp::relaxation(&with).solve_with_limit(1_000_000);
    (solved.status == LpStatus::Optimal).then_some(solved.objective)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("model path");
    let per_family: usize = args.next().map_or(200, |v| v.parse().unwrap());
    let mut problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
    ripsolve::presolve::presolve(&mut problem, 20);

    let mut lp = Lp::relaxation(&problem);
    let root = lp.solve_with_limit(1_000_000);
    if root.status != LpStatus::Optimal {
        println!("root relaxation did not finish");
        return;
    }
    println!("root {:.6}", root.objective);

    // Raw, before the violation filter, so a family that generates nothing can be told
    // apart from one whose cuts are all discarded.
    let raw = lp.gomory_cuts(&root.basis, 10_000);
    let violations: Vec<f64> = raw
        .iter()
        .map(|(c, lb)| {
            let activity: f64 = c.iter().map(|&(j, a)| a * root.x[j]).sum();
            lb - activity
        })
        .collect();
    let worst = violations.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    println!(
        "gomory raw: {} cuts generated, best violation {:.3e}, {} above 1e-6",
        raw.len(),
        worst,
        violations.iter().filter(|&&v| v > 1e-6).count()
    );

    let conflicts = cuts::Conflicts::of(&problem);
    let families: Vec<(&str, Vec<Cut>)> = vec![
        ("cover", cuts::separate(&problem, &root.x, per_family)),
        ("mir", cuts::separate_mir(&problem, &root.x, per_family)),
        (
            "clique",
            cuts::separate_cliques(&problem, &conflicts, &root.x, per_family),
        ),
        (
            "implagg",
            cuts::separate_implied_aggregations(&problem, &conflicts, &root.x, per_family),
        ),
        (
            "gomory",
            cuts::separate_gomory(&lp, &root.basis, &root.x, per_family),
        ),
        ("mod2", cuts::separate_mod2(&problem, &root.x, per_family)),
    ];

    let mut all: Vec<Cut> = Vec::new();
    for (name, found) in &families {
        let worst = found
            .iter()
            .map(|c| c.violation(&root.x))
            .fold(0.0f64, f64::max);
        let sizes: Vec<usize> = found.iter().map(|c| c.coefficients.len()).collect();
        let (smin, smax) = (
            sizes.iter().copied().min().unwrap_or(0),
            sizes.iter().copied().max().unwrap_or(0),
        );
        let after = re_solve(&problem, found);
        println!(
            "{name:<8} {:>4} cuts, size {smin}-{smax}, worst violation {worst:.6}, bound {}",
            found.len(),
            match after {
                Some(v) => format!("{v:.6}"),
                None => "did not solve".to_string(),
            }
        );
        all.extend(found.iter().cloned());
    }
    println!(
        "all      {:>4} cuts, bound {}",
        all.len(),
        match re_solve(&problem, &all) {
            Some(v) => format!("{v:.6}"),
            None => "did not solve".to_string(),
        }
    );
}
