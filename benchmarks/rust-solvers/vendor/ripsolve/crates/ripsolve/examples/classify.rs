//! Classify local MIPLIB instances by variable type.
//!
//! Prints one line per model: name, rows, columns, nonzeros, and the counts of binary,
//! general-integer and continuous columns. A pure binary program is one where the last
//! two are both zero.
use ripsolve::Problem;

fn main() {
    for path in std::env::args().skip(1) {
        let name = std::path::Path::new(&path)
            .file_stem()
            .map_or_else(|| path.clone(), |s| s.to_string_lossy().into_owned());
        let problem = match Problem::from_file(std::path::Path::new(&path)) {
            Ok(problem) => problem,
            Err(error) => {
                println!("{name}\tunreadable\t{error}");
                continue;
            }
        };
        let (mut binary, mut general, mut continuous) = (0usize, 0usize, 0usize);
        for j in 0..problem.n_cols() {
            if !problem.is_integer(j) {
                continuous += 1;
            // Anything an integer column can only take zero or one for, which
            // includes a binary already fixed to either end. Testing for exactly
            // [0, 1] misreads those as general integers: decomp1 carries six columns
            // fixed at [1, 1] and MIPLIB still tags it binary, rightly.
            } else if problem.col_lb[j] >= 0.0 && problem.col_ub[j] <= 1.0 {
                binary += 1;
            } else {
                general += 1;
            }
        }
        println!(
            "{name}\t{}\t{}\t{}\t{binary}\t{general}\t{continuous}",
            problem.n_rows(),
            problem.n_cols(),
            problem.matrix.nnz(),
        );
    }
}
