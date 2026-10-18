//! Checked standalone factor/solve measurements, enabled only for benchmark builds.
use crate::{dense::DenseTail, parallel::Execution};
use lp_ip_craft::{Error, Result};
use std::time::Instant;
/// Separate measured scopes for three fresh numeric factorizations.
pub struct Sample {
    /// Sum of dense factor and forward/transpose solve time.
    pub seconds: f64,
    /// Includes construction, worker resources, checks and teardown owner scope.
    pub workflow_seconds: f64,
    /// Cold local resource acquisition time (sharing may make this warm).
    pub resource_seconds: f64,
    /// Maximum component error against independently constructed exact points.
    pub error: f64,
    /// Hash of every factor bit, computed outside native timing.
    pub checksum: u64,
}
/// Factor three changed dense matrices and check forward/transpose solutions.
/// This is actual delivered dense-LU code, not a synthetic arithmetic loop.
pub fn run(n: usize, threads: usize) -> Result<Sample> {
    if !(32..=2048).contains(&n) {
        return Err(Error::InvalidInput(
            "factor benchmark size must lie in 32..=2048".into(),
        ));
    }
    let workflow = Instant::now();
    let cold = Instant::now();
    let execution = Execution::new(threads).map_err(Error::Backend)?;
    let resource_seconds = cold.elapsed().as_secs_f64();
    let (mut seconds, mut error, mut checksum) = (0., 0_f64, 0xcbf29ce484222325_u64);
    let x: Vec<_> = (0..n).map(|i| ((i * 7) % 13) as f64 / 16.).collect();
    for variant in 0..3 {
        let mut matrix = crate::active::Active::default();
        matrix.reset(n);
        for j in 0..n {
            for i in 0..n {
                matrix.insert(
                    j,
                    i,
                    if i == j {
                        2. * n as f64 + variant as f64 / 8.
                    } else {
                        ((i * 37 + j * 19) % 17 + 1) as f64 / 16.
                    },
                );
            }
        }
        let cols = matrix.cols;
        let forward: Vec<f64> = (0..n)
            .map(|i| (0..n).map(|j| cols[j][i].value * x[j]).sum())
            .collect();
        let transpose: Vec<f64> = (0..n)
            .map(|j| (0..n).map(|i| cols[j][i].value * x[i]).sum())
            .collect();
        let mut a = vec![0.; n];
        let mut b = vec![0.; n];
        let t = Instant::now();
        let mut factor = DenseTail::factor(
            &cols,
            (0..n).rev().collect(),
            (0..n).collect(),
            n,
            &execution,
        )
        .ok_or_else(|| Error::Backend("known nonsingular benchmark factor rejected".into()))?;
        factor.ftran(&forward, &mut a);
        factor.btran(&transpose, &mut b);
        seconds += t.elapsed().as_secs_f64();
        for ((a, b), x) in a.iter().zip(&b).zip(&x) {
            error = error.max((a - x).abs()).max((b - x).abs());
        }
        for v in &factor.data {
            checksum ^= v.to_bits();
            checksum = checksum.wrapping_mul(0x100000001b3);
        }
    }
    if !error.is_finite() || error > 1e-9 {
        return Err(Error::Backend(format!(
            "factor benchmark independent error{error}"
        )));
    }
    drop(execution);
    Ok(Sample {
        seconds,
        workflow_seconds: workflow.elapsed().as_secs_f64(),
        resource_seconds,
        error,
        checksum,
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn fresh_matrix_checksums_and_known_solves_match() {
        let a = super::run(129, 1).unwrap();
        let b = super::run(129, 8).unwrap();
        assert_eq!(a.checksum, b.checksum);
        assert_eq!(a.error.to_bits(), b.error.to_bits());
        assert!(
            a.seconds > 0. && a.workflow_seconds >= a.seconds && b.workflow_seconds >= b.seconds
        );
    }
}
