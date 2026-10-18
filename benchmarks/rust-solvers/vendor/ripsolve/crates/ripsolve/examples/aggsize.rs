//! What doubleton aggregation removes from a model, before anything else runs.
use ripsolve::Problem;
use ripsolve::aggregate;

fn main() {
    println!("{:<24} {:>8} {:>8} {:>9}   {:>8} {:>8} {:>9}", "model", "rows", "cols", "nnz", "rows", "cols", "nnz");
    for path in std::env::args().skip(1) {
        let name = std::path::Path::new(&path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let problem = Problem::from_file(std::path::Path::new(&path)).unwrap();
        match aggregate::aggregate(&problem, 1e-9) {
            Err(_) => println!("{name:<24} proved infeasible by aggregation"),
            Ok(None) => println!(
                "{name:<24} {:>8} {:>8} {:>9}   unchanged",
                problem.n_rows(), problem.n_cols(), problem.matrix.nnz()
            ),
            Ok(Some((smaller, map))) => { 
                let colstats = |q: &Problem| {
                    let mut v: Vec<usize> = (0..q.n_cols()).map(|j| q.matrix.column(j).0.len()).collect();
                    v.sort_unstable();
                    let mean = v.iter().sum::<usize>() as f64 / v.len().max(1) as f64;
                    (mean, v[v.len()/2], *v.last().unwrap_or(&0))
                };
                let (bm, bmed, bmax) = colstats(&problem);
                let (am, amed, amax) = colstats(&smaller);
                println!("{name:<20} column nnz  mean {bm:>6.1}->{am:>6.1}  median {bmed:>4}->{amed:>4}  max {bmax:>6}->{amax:>6}");
                println!(
                "{name:<24} {:>8} {:>8} {:>9} ->{:>8} {:>8} {:>9}   {} eliminated",
                problem.n_rows(), problem.n_cols(), problem.matrix.nnz(),
                smaller.n_rows(), smaller.n_cols(), smaller.matrix.nnz(), map.eliminated()
            ); }
        }
    }
}
