//! Optional basis capture/replay for numerical-kernel development.
//! Enable `basis-replay`; normal builds contain no filesystem capture code.
use crate::lu::Lu;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// One frozen basis with raw row/position right-hand sides.
#[derive(Debug)]
pub struct BasisCase {
    pub columns: Vec<Vec<(usize, f64)>>,
    pub rhs: Vec<Vec<f64>>,
}
impl BasisCase {
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let mut f = io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(b"LPIPBAS1")?;
        let mut word = |x: u64| f.write_all(&x.to_le_bytes());
        word(self.columns.len() as u64)?;
        for col in &self.columns {
            word(col.len() as u64)?;
            for &(i, v) in col {
                word(i as u64)?;
                word(v.to_bits())?;
            }
        }
        word(self.rhs.len() as u64)?;
        for rhs in &self.rhs {
            for &v in rhs {
                word(v.to_bits())?;
            }
        }
        f.flush()
    }
    pub fn read(path: &Path) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid basis replay file");
        if bytes.get(..8) != Some(b"LPIPBAS1") {
            return Err(invalid());
        }
        let mut at: usize = 8;
        let mut word = || -> io::Result<u64> {
            let end = at.checked_add(8).ok_or_else(invalid)?;
            let s = bytes.get(at..end).ok_or_else(invalid)?;
            at = end;
            Ok(u64::from_le_bytes(s.try_into().unwrap()))
        };
        let m = usize::try_from(word()?).map_err(|_| invalid())?;
        if m == 0 || m > bytes.len() / 8 {
            return Err(invalid());
        }
        let mut columns = Vec::with_capacity(m);
        let mut seen = vec![false; m];
        for _ in 0..m {
            let n = usize::try_from(word()?).map_err(|_| invalid())?;
            if n > m {
                return Err(invalid());
            }
            let mut col = Vec::with_capacity(n);
            for _ in 0..n {
                let i = usize::try_from(word()?).map_err(|_| invalid())?;
                let v = f64::from_bits(word()?);
                if i >= m || !v.is_finite() || seen[i] {
                    return Err(invalid());
                }
                seen[i] = true;
                col.push((i, v));
            }
            for &(i, _) in &col {
                seen[i] = false;
            }
            columns.push(col);
        }
        let count = usize::try_from(word()?).map_err(|_| invalid())?;
        if count == 0 || count > 64 {
            return Err(invalid());
        }
        let mut rhs = Vec::with_capacity(count);
        for _ in 0..count {
            let mut b = Vec::with_capacity(m);
            for _ in 0..m {
                let v = f64::from_bits(word()?);
                if !v.is_finite() {
                    return Err(invalid());
                }
                b.push(v);
            }
            rhs.push(b);
        }
        if at != bytes.len() {
            return Err(invalid());
        }
        Ok(Self { columns, rhs })
    }
}

pub(crate) fn capture(
    m: usize,
    mut column: impl FnMut(usize, &mut Vec<(usize, f64)>),
    rhs: &[&[f64]],
) {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let Some(dir) = std::env::var_os("LPIP_CAPTURE_DIR") else {
        return;
    };
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let selected =
        std::env::var("LPIP_CAPTURE_AT").unwrap_or_else(|_| "0,1,4,8,12,16,32,48,64,66".into());
    if !selected.split(',').any(|s| s.parse::<usize>() == Ok(n)) {
        return;
    }
    let columns = (0..m)
        .map(|p| {
            let mut col = Vec::new();
            column(p, &mut col);
            col
        })
        .collect();
    let mut rhs: Vec<_> = rhs
        .iter()
        .filter(|v| v.len() == m && v.iter().all(|v| v.is_finite()))
        .map(|v| v.to_vec())
        .collect();
    let mut unit = vec![0.; m];
    unit[m / 2] = 1.;
    rhs.push(unit);
    rhs.push((0..m).map(|i| if i % 67 == 0 { 1. } else { 0. }).collect());
    rhs.push(
        (0..m)
            .map(|i| ((i * 17 + 3) % 23) as f64 / 23. - 0.5)
            .collect(),
    );
    let result = std::fs::create_dir_all(&dir).and_then(|_| {
        BasisCase { columns, rhs }.write(&Path::new(&dir).join(format!("basis-{n:04}.bin")))
    });
    if let Err(e) = result {
        eprintln!("basis capture failed: {e}");
    }
}

/// Timed factorization and solve results. Residual validation is untimed.
#[derive(Debug)]
pub struct ReplayResult {
    pub factor_seconds: f64,
    pub solve_seconds: f64,
    pub entries: usize,
    pub dense_dimension: usize,
    pub pivot_fingerprint: u64,
    pub max_backward_error: f64,
}
/// Rebuild the saved matrix and validate every RHS in both orientations.
pub fn run(case: &BasisCase, cap: usize) -> Result<ReplayResult, String> {
    run_with_options(case, cap, 4, false)
}
/// Replay with explicit pivot-search and ordering settings.
pub fn run_with_options(
    case: &BasisCase,
    cap: usize,
    search: usize,
    ordered: bool,
) -> Result<ReplayResult, String> {
    if !(1..=64).contains(&search) {
        return Err("search must be between 1 and 64".into());
    }
    let m = case.columns.len();
    let start = Instant::now();
    let mut workspace = crate::active::FactorWorkspace::default();
    let mut lu = Lu::factor_with_workspace(
        m,
        |p, out| out.extend_from_slice(&case.columns[p]),
        cap,
        &mut workspace,
        crate::lu::FactorOptions {
            search,
            ordered,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let factor_seconds = start.elapsed().as_secs_f64();
    let entries = lu.entries();
    let (dense_dimension, pivot_fingerprint) = lu.replay_details();
    let mut solve_seconds = 0.;
    let mut error = 0.0_f64;
    for b in &case.rhs {
        for transpose in [false, true] {
            let mut x = b.clone();
            let start = Instant::now();
            if transpose {
                lu.btran(&mut x);
            } else {
                lu.ftran(&mut x);
            }
            solve_seconds += start.elapsed().as_secs_f64();
            let mut back = vec![0.; m];
            let mut scale = vec![1.; m];
            for (p, col) in case.columns.iter().enumerate() {
                for &(i, a) in col {
                    let (j, v) = if transpose {
                        (p, a * x[i])
                    } else {
                        (i, a * x[p])
                    };
                    back[j] += v;
                    scale[j] += v.abs();
                }
            }
            for i in 0..m {
                let e = (back[i] - b[i]).abs() / (scale[i] + b[i].abs());
                if !e.is_finite() {
                    return Err("nonfinite replay residual".into());
                }
                error = error.max(e);
            }
        }
    }
    Ok(ReplayResult {
        factor_seconds,
        solve_seconds,
        entries,
        dense_dimension,
        pivot_fingerprint,
        max_backward_error: error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_files_and_round_trip() {
        let path = std::env::temp_dir().join(format!("lpip-replay-{}.bin", std::process::id()));
        let case = BasisCase {
            columns: vec![vec![(0, 2.), (1, 1.)], vec![(1, 3.)]],
            rhs: vec![vec![1., 2.]],
        };
        case.write(&path).unwrap();
        let restored = BasisCase::read(&path).unwrap();
        assert!(run(&restored, 100).unwrap().max_backward_error < 1e-14);
        for bytes in [b"LPIPBAS1".as_slice(), b"bad".as_slice()] {
            std::fs::write(&path, bytes).unwrap();
            assert!(BasisCase::read(&path).is_err());
        }
        std::fs::remove_file(path).unwrap();
    }
}
