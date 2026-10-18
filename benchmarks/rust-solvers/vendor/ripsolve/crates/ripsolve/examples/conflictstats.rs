//! Report the size of the conflict graph a model yields.
use ripsolve::Problem;
use ripsolve::cuts::Conflicts;

fn main() {
    for path in std::env::args().skip(1) {
        let problem = match Problem::from_file(std::path::Path::new(&path)) {
            Ok(p) => p,
            Err(e) => {
                println!("{path}: read error {e}");
                continue;
            }
        };
        let name = std::path::Path::new(&path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let conflicts = Conflicts::of(&problem);
        println!(
            "{:<24} {:>6} rows {:>7} cols | edges {:>12} triangles {:>8} | cliques {:>6} longest {:>6}",
            name,
            problem.n_rows(),
            problem.n_cols(),
            conflicts.edges(),
            conflicts.triangles(),
            conflicts.clique_shape().0,
            conflicts.clique_shape().1
        );
    }
}
