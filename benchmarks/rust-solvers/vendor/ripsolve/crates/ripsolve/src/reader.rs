//! Reading problems from LP and MPS files.
//!
//! Parsing is delegated to `lp_parser_rs`; this module's job is the translation
//! into [`Problem`], range-form rows and a minimization-form objective.
//!
//! Column and row order follow the order the names appear in the source file.
//! `lp_parser_rs` keys its collections with `IndexMap`, so that order is stable,
//! and stability matters well beyond tidiness: column indices decide branching
//! order and degenerate pivot choices, so an order that varied between runs would
//! make node counts irreproducible on identical input.

use std::collections::HashSet;
use std::path::Path;

use lp_parser_rs::model::{ComparisonOp, Constraint, Sense as LpSense, VariableType};
use lp_parser_rs::parser::parse_file;
use lp_parser_rs::problem::LpProblem;

use crate::model::{Problem, RowSense, Sense, VarType};
use crate::sparse::SparseMatrix;

/// The magnitude from which a bound is read as infinite.
const INFINITE_BOUND: f64 = 1e30;

/// Headers that begin a list of integral variables in an LP file.
const INTEGRAL_HEADERS: [&str; 9] = [
    "binaries", "binary", "bin", "generals", "general", "gen", "integers", "integer", "int",
];
/// Headers that begin the bounds section.
const BOUNDS_HEADERS: [&str; 2] = ["bounds", "bound"];
/// Every other header. `subject` and `such` cover the two spellings of the
/// constraint header without matching a variable named `st`.
const OTHER_HEADERS: [&str; 10] = [
    "end", "maximize", "maximise", "minimize", "minimise", "max", "min", "subject", "such",
    "sos",
];

/// The lines of an LP file's sections that begin with one of `wanted`, comments
/// stripped. A header may carry content on its own line, so that part is kept too.
fn section_lines<'a>(text: &'a str, wanted: &[&str]) -> Vec<&'a str> {
    let mut lines = Vec::new();
    let mut collecting = false;
    for line in text.lines() {
        // Strip comments, which run from a backslash to the end of the line.
        let line = line.split('\\').next().unwrap_or("").trim();
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        let header = first.trim_end_matches(':').to_ascii_lowercase();
        let header = header.as_str();
        if wanted.contains(&header) {
            collecting = true;
            lines.push(line[first.len()..].trim());
        } else if INTEGRAL_HEADERS.contains(&header)
            || BOUNDS_HEADERS.contains(&header)
            || OTHER_HEADERS.contains(&header)
        {
            collecting = false;
        } else if collecting {
            lines.push(line);
        }
    }
    lines
}

/// Names declared integral by an LP file's `Binary`, `General` or `Integer`
/// sections, recovered from the source text.
///
/// This exists to work around `lp_parser_rs`: a `Bounds` section overwrites a
/// variable's type, so `x` declared under `General` and then bounded to `[0, 10]`
/// comes back as a plain double-bounded *continuous* variable, its integrality
/// gone. The bounds it reports are right; only the type is lost. Rather than
/// re-parse the file, this recovers the one fact the parser drops and leaves
/// everything else to it.
fn declared_integer(text: &str) -> HashSet<String> {
    section_lines(text, &INTEGRAL_HEADERS)
        .into_iter()
        .flat_map(str::split_whitespace)
        .map(str::to_string)
        .collect()
}

/// Names an LP file's `Bounds` section mentions, recovered from the source text.
///
/// Another workaround for `lp_parser_rs`, which reports a variable the file never
/// bounds as `Free`, the same as one the file declares free. The LP format gives
/// an undeclared variable `[0, inf)`, as Gurobi and CPLEX read it, so only a name
/// this finds may be taken at its word. Reading every `Free` as free turned
/// `Minimize dummy` over an otherwise bounded model into an unbounded one.
fn named_in_bounds(text: &str) -> HashSet<String> {
    section_lines(text, &BOUNDS_HEADERS)
        .into_iter()
        .flat_map(|line| line.split(|c: char| c.is_whitespace() || "<>=".contains(c)))
        .filter(|token| {
            !token.is_empty()
                && !token.eq_ignore_ascii_case("free")
                // Numbers, including `inf` and `-infinity`, which `f64` parses.
                && token.parse::<f64>().is_err()
        })
        .map(str::to_string)
        .collect()
}

/// Names declared integral by an MPS file, recovered from the source text.
///
/// The same workaround as [`declared_integer`], for the other format. MPS carries
/// integrality two ways and `lp_parser_rs` reports neither: `MARKER` lines bracket
/// runs of integer columns inside `COLUMNS`, and the `BOUNDS` section can mark a
/// column directly with `BV` (binary), `LI` or `UI` (integer bounds).
///
/// Losing this is not a small matter. A model whose integrality vanishes solves as
/// a pure LP and reports the relaxation as a proven optimum, silently, and with a
/// better objective than the true one, which is exactly what makes it hard to
/// notice. MIPLIB's `flugpl` came back at 1167185.73 against a true optimum of
/// 1201500 before this existed.
/// MPS sections that change what the model means, and that the parser drops.
///
/// Skipping one of these does not produce an approximate answer, it produces a
/// confident answer to a different question. `INDICATORS` is the case that found this:
/// its rows appear in `ROWS` and `COLUMNS` like any other, and the section is what
/// says they apply only when their binary is set. Dropping it leaves those conditional
/// constraints standing unconditionally, and on MIPLIB's cvrpsimple2i that turned a
/// model with optimum 34 into one ripsolve proved infeasible, in half a millisecond
/// and with no indication anything was wrong.
const UNSUPPORTED_SECTIONS: [&str; 6] = [
    "INDICATORS",
    "SOS",
    "QUADOBJ",
    "QMATRIX",
    "QCMATRIX",
    "LAZYCONS",
];

/// The first unsupported section header in an MPS file, if any.
///
/// Only a header in column one counts, so a row or column named for one of these is
/// not mistaken for the section itself.
fn unsupported_section(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.starts_with('*') || line.starts_with([' ', '\t']) {
            continue;
        }
        let Some(header) = line.split_whitespace().next() else {
            continue;
        };
        let upper = header.to_ascii_uppercase();
        if UNSUPPORTED_SECTIONS.contains(&upper.as_str()) {
            return Some(upper);
        }
    }
    None
}

fn declared_integer_mps(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut section = "";
    let mut inside_marker = false;

    for line in text.lines() {
        if line.starts_with('*') {
            continue;
        }
        // A section header starts in column one; data lines are indented.
        if !line.starts_with([' ', '\t']) {
            section = line.split_whitespace().next().unwrap_or("");
            inside_marker = false;
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }

        match section {
            "COLUMNS" => {
                // MARKER lines carry INTORG/INTEND rather than matrix data.
                if line.contains("'MARKER'") {
                    if line.contains("'INTORG'") {
                        inside_marker = true;
                    } else if line.contains("'INTEND'") {
                        inside_marker = false;
                    }
                    continue;
                }
                if inside_marker {
                    names.insert(fields[0].to_string());
                }
            }
            // `<type> <bound-set> <column> [value]`; three of the bound types imply
            // integrality regardless of any MARKER block.
            "BOUNDS" if matches!(fields[0], "BV" | "LI" | "UI") && fields.len() >= 3 => {
                names.insert(fields[2].to_string());
            }
            _ => {}
        }
    }
    names
}

/// Failure to turn a model file into a [`Problem`].
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("could not read or parse model file: {0}")]
    Parse(String),
    #[error("unrecognised model file extension {0:?}; expected .lp or .mps")]
    UnknownFormat(String),
    #[error("the model has no objective function")]
    NoObjective,
    #[error("variable {0:?} is {1}, which ripsolve does not support")]
    UnsupportedVariable(String, String),
    #[error("integer variable {0:?} has the fractional bound {1}")]
    FractionalIntegerBound(String, f64),
    #[error("constraint {0:?} is a special ordered set, which ripsolve does not support")]
    SosConstraint(String),
    #[error(
        "the model has an MPS {0} section, which ripsolve does not support; \
         solving it would mean solving a different model"
    )]
    UnsupportedSection(String),
}

/// Which parser to use for a model file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Lp,
    Mps,
}

impl Format {
    /// Infer the format from a path's extension, case-insensitively.
    pub fn from_path(path: &Path) -> Result<Format, ReadError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("lp") => Ok(Format::Lp),
            Some(e) if e.eq_ignore_ascii_case("mps") => Ok(Format::Mps),
            other => Err(ReadError::UnknownFormat(other.unwrap_or("").to_string())),
        }
    }
}

impl Problem {
    /// Read a mixed-integer program from a file, choosing the parser by extension.
    pub fn from_file(path: &Path) -> Result<Problem, ReadError> {
        Problem::from_file_as(path, Format::from_path(path)?)
    }

    /// Read a mixed-integer program from a file in an explicitly given format.
    pub fn from_file_as(path: &Path, format: Format) -> Result<Problem, ReadError> {
        let content = parse_file(path).map_err(|e| ReadError::Parse(e.to_string()))?;
        Problem::from_text(&content, format)
    }

    /// Read a mixed-integer program from the text of a file in the given format.
    pub fn from_text(content: &str, format: Format) -> Result<Problem, ReadError> {
        let lp = match format {
            Format::Lp => LpProblem::parse(content),
            Format::Mps => LpProblem::parse_mps(content),
        }
        .map_err(|e| ReadError::Parse(e.to_string()))?;
        // The LP source is consulted only to recover what the parser drops: which
        // names are integral (see `declared_integer`) and which `Free` columns the
        // file really declared free (see `named_in_bounds`). MPS needs only the
        // first; its parser gives an unbounded column the format's default itself.
        let (integral, bounded) = match format {
            Format::Lp => (declared_integer(content), Some(named_in_bounds(content))),
            Format::Mps => {
                if let Some(section) = unsupported_section(content) {
                    return Err(ReadError::UnsupportedSection(section));
                }
                (declared_integer_mps(content), None)
            }
        };
        Problem::from_parsed(&lp, &integral, bounded.as_ref())
    }

    /// Translate an already-parsed [`LpProblem`].
    ///
    /// A `Maximize` objective is negated so the solver always works in
    /// minimization form; [`Problem::sense`] records what was asked for and
    /// [`Problem::objective_value`] converts results back.
    pub fn from_lp(lp: &LpProblem) -> Result<Problem, ReadError> {
        Problem::from_parsed(lp, &HashSet::new(), None)
    }

    /// As [`Problem::from_lp`], with extra names known to be integral.
    pub fn from_lp_with_integrality(
        lp: &LpProblem,
        integral: &HashSet<String>,
    ) -> Result<Problem, ReadError> {
        Problem::from_parsed(lp, integral, None)
    }

    /// The translation itself. `bounded`, when given, names the columns an LP
    /// file's `Bounds` section mentions; a `Free` column outside it was never
    /// declared at all and takes the format's default `[0, inf)`. Without it every
    /// `Free` column is taken as declared.
    fn from_parsed(
        lp: &LpProblem,
        integral: &HashSet<String>,
        bounded: Option<&HashSet<String>>,
    ) -> Result<Problem, ReadError> {
        // MPS marks the objective as a free row, and files routinely carry more than
        // one: the first is the objective and the rest are free rows kept for other
        // purposes. Every mainstream solver takes the first and ignores the others,
        // and MIPLIB instances rely on it, so rejecting the file would be a reader
        // limitation rather than a real restriction. Both formats preserve source
        // order, so "first" is well defined.
        let objective = lp
            .objectives
            .values()
            .next()
            .ok_or(ReadError::NoObjective)?;
        let name = |id| lp.interner.resolve(id).to_string();

        // Column types and bounds, from whichever section declared them.
        //
        // A variable's *type* and its *bounds* are separate in both formats: an
        // integer declared with no bounds is `[0, inf)`, a binary one is an integer
        // pinned to `[0, 1]`, and a bounds section can narrow either. Reading them
        // as one thing is how a model silently becomes the wrong model.
        let mut col_names = Vec::with_capacity(lp.variables.len());
        let mut col_type = Vec::with_capacity(lp.variables.len());
        let mut col_lb = Vec::with_capacity(lp.variables.len());
        let mut col_ub = Vec::with_capacity(lp.variables.len());

        for var in lp.variables.values() {
            let label = name(var.name);
            let (mut kind, lo, hi) = match &var.var_type {
                VariableType::Binary => (VarType::Integer, 0.0, 1.0),
                VariableType::Integer | VariableType::General => {
                    (VarType::Integer, 0.0, f64::INFINITY)
                }
                VariableType::Free if bounded.is_some_and(|b| !b.contains(&label)) => {
                    (VarType::Continuous, 0.0, f64::INFINITY)
                }
                VariableType::Free => (VarType::Continuous, f64::NEG_INFINITY, f64::INFINITY),
                VariableType::LowerBound(lo) => (VarType::Continuous, *lo, f64::INFINITY),
                VariableType::UpperBound(hi) => (VarType::Continuous, 0.0, *hi),
                VariableType::DoubleBound(lo, hi) => (VarType::Continuous, *lo, *hi),
                other => {
                    return Err(ReadError::UnsupportedVariable(label, other.to_string()));
                }
            };
            // Gurobi and CPLEX read a bound at or beyond 1e30 as infinite, and files
            // write it that way. Kept finite, it is a number presolve may fix a
            // column at.
            let lo = if lo <= -INFINITE_BOUND { f64::NEG_INFINITY } else { lo };
            let hi = if hi >= INFINITE_BOUND { f64::INFINITY } else { hi };
            // A `Bounds` entry replaces the declared type, so the section lists are
            // the only surviving evidence that the column is integral.
            if integral.contains(&label) {
                kind = VarType::Integer;
            }
            if kind == VarType::Integer {
                for bound in [lo, hi] {
                    if bound.is_finite() && (bound - bound.round()).abs() > 1e-9 {
                        return Err(ReadError::FractionalIntegerBound(label, bound));
                    }
                }
            }
            col_names.push(label);
            col_type.push(kind);
            col_lb.push(lo);
            col_ub.push(hi);
        }

        // Position within the IndexMap is the column index, so a name lookup is just
        // `get_index_of`, no side table to build or keep consistent.
        let col_of = |id| {
            lp.variables
                .get_index_of(&id)
                .expect("variable is interned and present")
        };

        let negate = lp.sense == LpSense::Maximize;
        let flip = |v: f64| if negate { -v } else { v };

        let n_cols = col_names.len();
        let mut obj = vec![0.0f64; n_cols];
        for coeff in &objective.coefficients {
            // `+=`: both formats permit a variable to appear more than once in an
            // expression, with the terms accumulating.
            obj[col_of(coeff.name)] += flip(coeff.value);
        }

        let n_rows = lp.constraints.len();
        let mut triplets: Vec<(usize, usize, f64)> = Vec::new();
        let mut row_lb = Vec::with_capacity(n_rows);
        let mut row_ub = Vec::with_capacity(n_rows);
        let mut row_names = Vec::with_capacity(n_rows);

        for (i, constraint) in lp.constraints.values().enumerate() {
            match constraint {
                Constraint::Standard {
                    name: row_name,
                    coefficients,
                    operator,
                    rhs,
                    ..
                } => {
                    let sense = match operator {
                        // Neither format has strict inequalities; some writers emit
                        // them anyway and every solver reads them as non-strict.
                        ComparisonOp::GTE | ComparisonOp::GT => RowSense::Ge,
                        ComparisonOp::LTE | ComparisonOp::LT => RowSense::Le,
                        ComparisonOp::EQ => RowSense::Eq,
                    };
                    let (lo, hi) = sense.bounds(*rhs);
                    row_lb.push(lo);
                    row_ub.push(hi);
                    row_names.push(name(*row_name));
                    for coeff in coefficients {
                        triplets.push((i, col_of(coeff.name), coeff.value));
                    }
                }
                Constraint::SOS { name: row_name, .. } => {
                    return Err(ReadError::SosConstraint(name(*row_name)));
                }
            }
        }

        Ok(Problem {
            name: lp.name.clone().unwrap_or_else(|| "unnamed".to_string()),
            sense: if negate {
                Sense::Maximize
            } else {
                Sense::Minimize
            },
            obj,
            obj_offset: flip(objective.constant),
            matrix: SparseMatrix::from_triplets(n_rows, n_cols, triplets),
            row_lb,
            row_ub,
            col_lb,
            col_ub,
            col_type,
            col_names,
            row_names,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MPS file whose conditional row is marked by an INDICATORS section.
    ///
    /// `c2` binds only when `z` is 1. Read without the section it stands
    /// unconditionally, which is how a feasible model comes to look infeasible.
    const INDICATOR_MPS: &str = "\
NAME          cond
ROWS
 N  obj
 G  c1
 G  c2
COLUMNS
    x         obj       1.0        c1        1.0
    y         obj       1.0        c1        1.0
    x         c2        1.0
    y         c2        1.0
RHS
    rhs       c1        1.0        c2        2.0
BOUNDS
 UI bnd       x         1.0
 UI bnd       y         1.0
 BV bnd       z
INDICATORS
 IF c2        z         1
ENDATA
";

    #[test]
    fn an_indicator_section_is_refused_rather_than_dropped() {
        // Dropping the section does not approximate the model, it replaces it with a
        // different one and then answers confidently. On MIPLIB's cvrpsimple2i that
        // turned an instance with optimum 34 into a proof of infeasibility.
        assert_eq!(
            unsupported_section(INDICATOR_MPS).as_deref(),
            Some("INDICATORS")
        );
    }

    #[test]
    fn ordinary_mps_files_are_not_mistaken_for_unsupported_ones() {
        // Only a section header in column one counts. A row or column that happens to
        // share a name with one must not trip the check.
        let ordinary = "\
NAME          plain
ROWS
 N  obj
 L  SOS
COLUMNS
    INDICATORS  obj       1.0        SOS       1.0
RHS
    rhs       SOS       1.0
ENDATA
";
        assert_eq!(unsupported_section(ordinary), None);
    }

    fn parse(text: &str) -> Result<Problem, ReadError> {
        let lp = LpProblem::parse(text).map_err(|e| ReadError::Parse(e.to_string()))?;
        Problem::from_lp(&lp)
    }

    const SIMPLE: &str = "\
Minimize
 obj: 3 x1 + 2 x2 + 4 x10
Subject To
 c1: x1 + x2 >= 1
 c2: x2 + x10 <= 1
 c3: x1 + x10 = 1
Binary
 x1 x2 x10
End
";

    #[test]
    fn reads_columns_in_file_order() {
        let p = parse(SIMPLE).unwrap();
        assert_eq!(p.col_names, ["x1", "x2", "x10"]);
        assert_eq!(p.obj, [3.0, 2.0, 4.0]);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn converts_each_sense_to_range_bounds() {
        let p = parse(SIMPLE).unwrap();
        let row = |name: &str| {
            let i = p.row_names.iter().position(|n| n == name).unwrap();
            (p.row_lb[i], p.row_ub[i])
        };
        assert_eq!(row("c1"), (1.0, f64::INFINITY));
        assert_eq!(row("c2"), (f64::NEG_INFINITY, 1.0));
        assert_eq!(row("c3"), (1.0, 1.0));
    }

    #[test]
    fn maximize_is_negated_and_reported_back() {
        let p =
            parse("Maximize\n obj: 3 x1\nSubject To\n c1: x1 <= 1\nBinary\n x1\nEnd\n").unwrap();
        assert_eq!(p.sense, Sense::Maximize);
        // Stored negated for the minimizing solver...
        assert_eq!(p.obj, [-3.0]);
        // ...and converted back on the way out.
        assert_eq!(p.objective_value(-3.0), 3.0);
    }

    #[test]
    fn repeated_terms_accumulate() {
        let p =
            parse("Minimize\n obj: 2 x1 + 3 x1\nSubject To\n c1: x1 + x1 >= 1\nBinary\n x1\nEnd\n")
                .unwrap();
        assert_eq!(p.obj, [5.0]);
        assert_eq!(p.matrix.column(0), (&[0usize][..], &[2.0][..]));
    }

    #[test]
    fn order_is_stable_across_repeated_parses() {
        // Column index drives branching order, so an unstable order would make node
        // counts irreproducible on identical input.
        let first = parse(SIMPLE).unwrap();
        for _ in 0..16 {
            let again = parse(SIMPLE).unwrap();
            assert_eq!(first.col_names, again.col_names);
            assert_eq!(first.row_names, again.row_names);
            assert_eq!(first.matrix, again.matrix);
        }
    }

    #[test]
    fn reads_a_general_integer_variable() {
        // No bounds section, so a General integer is [0, inf), not binary.
        let p = parse("Minimize\n obj: x1\nSubject To\n c1: x1 >= 1\nGeneral\n x1\nEnd\n").unwrap();
        assert!(p.is_integer(0));
        assert!(!p.is_binary(0), "an unbounded integer is not binary");
        assert_eq!((p.col_lb[0], p.col_ub[0]), (0.0, f64::INFINITY));
    }

    #[test]
    fn an_undeclared_continuous_variable_is_nonnegative() {
        // The LP format's default is [0, inf). Read as free, `Minimize dummy` over a
        // column no row mentions was unbounded, and a sudoku file whose objective
        // was a placeholder hit the node limit instead of solving.
        let text = "Minimize\n dummy\nSubject To\n c1: x + y = 1\nBinary\n x y\nEnd\n";
        let p = Problem::from_text(text, Format::Lp).unwrap();
        assert_eq!(p.col_names[0], "dummy");
        assert_eq!((p.col_lb[0], p.col_ub[0]), (0.0, f64::INFINITY));
    }

    #[test]
    fn a_variable_the_bounds_section_frees_stays_free() {
        let text = "Minimize\n a + b + c + d\nSubject To\n c1: a + b + c + d >= -10\n\
                    Bounds\n a free\n b >= -inf\n -inf <= c <= +inf\n d <= 3\nEnd\n";
        let p = Problem::from_text(text, Format::Lp).unwrap();
        let bounds: Vec<_> = (0..4).map(|j| (p.col_lb[j], p.col_ub[j])).collect();
        let free = (f64::NEG_INFINITY, f64::INFINITY);
        // `d <= 3` sets only the upper bound, so the lower keeps its default.
        assert_eq!(bounds, vec![free, free, free, (0.0, 3.0)]);
    }

    #[test]
    fn a_bound_of_1e30_is_infinite() {
        let text = "Minimize\n x\nSubject To\n c1: x + y >= 1\nBounds\n\
                    x >= -1e30\n -1e30 <= y <= 1e30\nEnd\n";
        let p = Problem::from_text(text, Format::Lp).unwrap();
        assert_eq!(p.col_lb, vec![f64::NEG_INFINITY; 2]);
        assert_eq!(p.col_ub, vec![f64::INFINITY; 2]);
    }

    #[test]
    fn a_binary_declaration_is_an_integer_bounded_to_one() {
        let p = parse(SIMPLE).unwrap();
        assert!((0..p.n_cols()).all(|j| p.is_binary(j)));
        assert_eq!(p.col_ub, vec![1.0; p.n_cols()]);
    }

    #[test]
    fn rejects_a_model_with_no_objective() {
        // `Subject To` alone is not a complete LP file, so build the case directly.
        let empty = LpProblem::default();
        assert!(matches!(
            Problem::from_lp(&empty),
            Err(ReadError::NoObjective)
        ));
    }

    #[test]
    fn format_is_inferred_from_extension_case_insensitively() {
        assert_eq!(Format::from_path(Path::new("m.lp")).unwrap(), Format::Lp);
        assert_eq!(Format::from_path(Path::new("m.MPS")).unwrap(), Format::Mps);
        assert!(matches!(
            Format::from_path(Path::new("m.txt")),
            Err(ReadError::UnknownFormat(_))
        ));
    }
}
