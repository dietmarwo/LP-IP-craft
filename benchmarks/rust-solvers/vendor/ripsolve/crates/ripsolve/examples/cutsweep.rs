use ripsolve::Problem;
use ripsolve::search::{self, Options};
use std::time::Instant;

/// Cut budgets to compare, as (rounds, cuts per round).
const BUDGETS: [(usize, usize); 3] = [(0, 0), (1, 8), (3, 32)];

/// Reproduces the measurement behind `Options::cut_rounds` defaulting to zero.
/// Paths outside `samples/` are local generated instances; those cases are skipped
/// when the files are absent.
fn main() {
    let home = std::env::var("HOME").unwrap();
    let cases = [
        ("v032c032", "samples/v032c032.lp".to_string()),
        ("v048c048", "samples/v048c048.lp".to_string()),
        ("v048c128", "samples/v048c128.lp".to_string()),
        ("v064c064", "samples/v064c064.lp".to_string()),
        ("v064c200", "samples/v064c200.mps".to_string()),
        (
            "v081c162n009",
            format!("{home}/repos/bip-gen/v081c162n009.lp"),
        ),
        (
            "v081c162n018",
            format!("{home}/repos/bip-gen/v081c162n018.lp"),
        ),
        (
            "v128c256n100",
            format!("{home}/repos/bip-gen/v128c256n100.lp"),
        ),
        (
            "v256c256n100",
            format!("{home}/repos/bip-gen/v256c256n100.lp"),
        ),
        (
            "v128c1000n100",
            format!("{home}/repos/bip-gen/v128c1000n100.lp"),
        ),
        ("mkp_200", "bench/out/mkp_200.lp".to_string()),
    ];
    println!("time and nodes per cut budget (rounds x cuts per round), best of three\n");
    print!("{:16}", "instance");
    for (r, p) in BUDGETS {
        print!("{:>18}", format!("{r}x{p}"));
    }
    println!();
    for (name, path) in cases {
        let path = std::path::Path::new(&path);
        if !path.exists() {
            continue;
        }
        let p = Problem::from_file(path).unwrap();
        print!("{name:16}");
        for (rounds, per) in BUDGETS {
            let o = Options {
                cut_rounds: rounds,
                cuts_per_round: per,
                threads: 1,
                ..Options::default()
            };
            let mut best = f64::MAX;
            let mut nodes = 0;
            for _ in 0..3 {
                let t = Instant::now();
                let s = search::solve(&p, o);
                best = best.min(t.elapsed().as_secs_f64());
                nodes = s.nodes;
            }
            print!("{:>18}", format!("{:.2}s/{}n", best, nodes));
        }
        println!();
    }
}
