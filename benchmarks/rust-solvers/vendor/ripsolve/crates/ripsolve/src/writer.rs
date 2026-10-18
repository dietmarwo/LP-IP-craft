//! Writing a [`Problem`] in CPLEX LP format, to hand to another solver or to keep
//! a model for diagnosis.
//!
//! The reader's crate, `lp_parser_rs`, has a writer of its own, but its variable
//! model gives a column one type that is either integral or bounded, never both, so
//! an integer column with bounds (or a binary fixed at zero) would come out as a
//! different model. This writer states every column's bounds and lists the integral
//! ones by name, the two things being separate in the format as they are here.
//!
//! What it writes reads back with [`Problem::from_text`] as the same model, with one
//! exception: a row bounded on both sides, which the format cannot state, becomes
//! two rows, `<name>_lo` and `<name>_hi`. Names are written as they are, so they
//! must already be valid LP names, as any read from a file are.

use std::fmt::Write as _;

use crate::model::{Problem, Sense};

/// Terms per line before an expression wraps; some readers limit line length.
const TERMS_PER_LINE: usize = 8;

impl Problem {
    /// The problem in LP format.
    pub fn to_lp(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "\\ {}", self.name);
        // The stored objective is in minimization form; undo that for the file.
        let (heading, sign) = match self.sense {
            Sense::Minimize => ("Minimize", 1.0),
            Sense::Maximize => ("Maximize", -1.0),
        };
        let _ = writeln!(out, "{heading}");
        let objective: Vec<(usize, f64)> = self
            .obj
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c != 0.0)
            .map(|(j, &c)| (j, sign * c))
            .collect();
        out.push_str(" obj:");
        match self.col_names.first() {
            // An objective must name something, even a zero one.
            Some(first) if objective.is_empty() => {
                let _ = write!(out, " 0 {first}");
            }
            _ => self.expression(&mut out, &objective),
        }
        if self.obj_offset != 0.0 {
            let _ = write!(out, " {}", signed(sign * self.obj_offset));
        }
        out.push('\n');

        out.push_str("Subject To\n");
        let rows = self.matrix.to_csr();
        for i in 0..self.n_rows() {
            let (cols, vals) = rows.column(i);
            let terms: Vec<(usize, f64)> =
                cols.iter().copied().zip(vals.iter().copied()).collect();
            let (lb, ub) = (self.row_lb[i], self.row_ub[i]);
            let name = &self.row_names[i];
            let mut row = |label: &str, op: &str, rhs: f64| {
                let _ = write!(out, " {label}:");
                if terms.is_empty() {
                    // An empty row still has to say something; it holds or it does not.
                    if let Some(first) = self.col_names.first() {
                        let _ = write!(out, " 0 {first}");
                    }
                } else {
                    self.expression(&mut out, &terms);
                }
                let _ = writeln!(out, " {op} {}", number(rhs));
            };
            match (lb.is_finite(), ub.is_finite()) {
                _ if lb == ub => row(name, "=", lb),
                (true, true) => {
                    row(&format!("{name}_lo"), ">=", lb);
                    row(&format!("{name}_hi"), "<=", ub);
                }
                (true, false) => row(name, ">=", lb),
                (false, true) => row(name, "<=", ub),
                // A free row constrains nothing.
                (false, false) => {}
            }
        }

        // Every column's range is stated, so nothing rests on the format's default
        // lower bound of zero.
        out.push_str("Bounds\n");
        for (j, name) in self.col_names.iter().enumerate() {
            let (lo, hi) = (self.col_lb[j], self.col_ub[j]);
            let _ = match (lo.is_finite(), hi.is_finite()) {
                _ if lo == hi => writeln!(out, " {name} = {}", number(lo)),
                (true, true) => writeln!(out, " {} <= {name} <= {}", number(lo), number(hi)),
                (true, false) => writeln!(out, " {name} >= {}", number(lo)),
                (false, true) => writeln!(out, " -inf <= {name} <= {}", number(hi)),
                (false, false) => writeln!(out, " {name} free"),
            };
        }
        let integers: Vec<&String> = self
            .col_names
            .iter()
            .enumerate()
            .filter(|&(j, _)| self.is_integer(j))
            .map(|(_, name)| name)
            .collect();
        if !integers.is_empty() {
            out.push_str("General\n");
            for line in integers.chunks(TERMS_PER_LINE) {
                for name in line {
                    let _ = write!(out, " {name}");
                }
                out.push('\n');
            }
        }
        out.push_str("End\n");
        out
    }

    /// ` + 3 x - 2 y …`, wrapping long expressions onto further lines.
    fn expression(&self, out: &mut String, terms: &[(usize, f64)]) {
        for (k, &(j, a)) in terms.iter().enumerate() {
            if k > 0 && k % TERMS_PER_LINE == 0 {
                out.push_str("\n   ");
            }
            let _ = write!(out, " {} {}", signed(a), self.col_names[j]);
        }
    }
}

/// `3` → `+ 3`, `-2.5` → `- 2.5`.
fn signed(v: f64) -> String {
    if v < 0.0 {
        format!("- {}", number(-v))
    } else {
        format!("+ {}", number(v))
    }
}

/// A finite number exactly, with an exponent when large or small: Rust's shortest
/// round-trip form, which every LP reader accepts.
fn number(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e15 || v.abs() < 1e-5) {
        format!("{v:e}")
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Builder, RowSense, Sense};
    use crate::search::{self, Options};
    use crate::{Format, Problem, VarType};

    /// Every kind of column and row the writer has to state.
    fn sample() -> Problem {
        let mut b = Builder::new(Sense::Maximize).named("sample");
        let x = b.binary("x");
        let fixed = b.column("fixed", 0.0, 0.0, VarType::Integer);
        let n = b.integer("n", -2.0, 5.0);
        let c = b.continuous("c", f64::NEG_INFINITY, 4.5);
        let f = b.continuous("f", f64::NEG_INFINITY, f64::INFINITY);
        b.objective(&[(x, 3.0), (fixed, 10.0), (n, 2.0), (c, 1.0), (f, 0.25)]);
        b.row(&[(x, 2.0), (n, 1.0)], RowSense::Le, 8.0);
        b.row(&[(c, 1.0), (f, -1.0)], RowSense::Eq, 1.0);
        b.range(&[(n, 1.0), (c, 1.0)], -3.0, 7.5);
        b.row(&[(f, 1.0)], RowSense::Le, 1e-7);
        b.build()
    }

    fn read_back(p: &Problem) -> Problem {
        let text = p.to_lp();
        Problem::from_text(&text, Format::Lp).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    #[test]
    fn columns_read_back_as_written() {
        let p = sample();
        let q = read_back(&p);
        assert_eq!(q.sense, p.sense);
        for (j, name) in p.col_names.iter().enumerate() {
            let k = q.col_names.iter().position(|n| n == name).unwrap();
            assert_eq!(q.col_type[k], p.col_type[j], "{name}");
            assert_eq!((q.col_lb[k], q.col_ub[k]), (p.col_lb[j], p.col_ub[j]), "{name}");
            assert_eq!(q.obj[k], p.obj[j], "{name}");
        }
        // The range row becomes two.
        assert_eq!(q.n_rows(), p.n_rows() + 1);
        assert!(q.row_names.iter().any(|n| n.ends_with("_lo")));
    }

    #[test]
    fn the_model_read_back_has_the_same_optimum() {
        let p = sample();
        let q = read_back(&p);
        let a = search::solve(&p, Options::default());
        let b = search::solve(&q, Options::default());
        assert!(a.objective.is_some());
        assert_eq!(a.objective, b.objective);
    }
}
