use lp_ip_craft::*;
use lp_ip_craft_native::*;

// Lossless binary64/CSR copy of docs/packing-mip/corpus/25fv47.json.
// Kept binary so the dependency-free Rust tests need no JSON parser.
fn fixture() -> ModelData {
    let bytes = include_bytes!("data/25fv47.bin");
    assert_eq!(&bytes[..8], b"LPIP25FV");
    let mut words = bytes[8..]
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let (n, m, nz) = (
        words.next().unwrap() as usize,
        words.next().unwrap() as usize,
        words.next().unwrap() as usize,
    );
    let mut floats = |len| {
        words
            .by_ref()
            .take(len)
            .map(f64::from_bits)
            .collect::<Vec<_>>()
    };
    let objective = floats(n);
    let variables = Bounds::new(floats(n), floats(n));
    let rows = Bounds::new(floats(m), floats(m));
    let values = floats(nz);
    let indptr = words.by_ref().take(m + 1).map(|v| v as usize).collect();
    let indices = words.by_ref().take(nz).map(|v| v as usize).collect();
    assert!(words.next().is_none());
    ModelData {
        sense: Sense::Minimize,
        objective,
        objective_offset: 0.,
        variables,
        rows,
        kinds: vec![VariableKind::Continuous; n],
        matrix: CsrMatrix::new(m, n, indptr, indices, values),
    }
}

#[test]
fn phase_one_unperturbed_classification_and_progress_restarts() {
    for variant in 0..4 {
        let mut d = fixture();
        let (n, m) = (d.objective.len(), d.rows.lower.len());
        if variant == 1 || variant == 3 {
            d.objective.reverse();
            d.variables.lower.reverse();
            d.variables.upper.reverse();
            for j in &mut d.matrix.pattern.indices {
                *j = n - 1 - *j;
            }
            for i in 0..m {
                let range = d.matrix.pattern.indptr[i]..d.matrix.pattern.indptr[i + 1];
                d.matrix.pattern.indices[range.clone()].reverse();
                d.matrix.values[range].reverse();
            }
        }
        if variant >= 2 {
            let old = d.matrix.clone();
            let mut ptr = vec![0];
            let mut indices = vec![];
            let mut values = vec![];
            d.rows.lower.reverse();
            d.rows.upper.reverse();
            for i in 0..m {
                let scale = 2_f64.powi((i % 9) as i32 - 4);
                d.rows.lower[i] *= scale;
                d.rows.upper[i] *= scale;
                let k = m - 1 - i;
                for e in old.pattern.indptr[k]..old.pattern.indptr[k + 1] {
                    indices.push(old.pattern.indices[e]);
                    values.push(old.values[e] * scale);
                }
                ptr.push(values.len());
            }
            d.matrix = CsrMatrix::new(m, n, ptr, indices, values);
        }
        let model = Model::new(d).unwrap();
        let run = |cap| {
            NativeSolver::new(NativeOptions {
                decompose: variant % 2 == 0,
                iteration_limit: cap,
                ..Default::default()
            })
            .solve(
                &model,
                &Options::default(),
                None,
                &CancellationToken::default(),
            )
            .unwrap()
        };
        let r = run(None);
        assert_eq!(
            r.status,
            Status::Optimal,
            "variant {variant}: {}",
            r.message
        );
        assert!((r.evaluation.as_ref().unwrap().objective - 5501.845888286748).abs() < 1e-7);
        let residual = check::lp_residuals(
            &model,
            r.candidate.as_ref().unwrap(),
            r.duals.as_ref().unwrap(),
        )
        .unwrap();
        assert!(
            residual.primal < 1e-7 && residual.dual_sign < 1e-7 && residual.duality_gap < 1e-7,
            "{residual:?}"
        );
        let limited = run(Some(950));
        assert_eq!(limited.status, Status::IterationLimit);
        assert!(limited.iterations > 0 && limited.iterations <= 950);
        if variant % 2 == 1 {
            assert_eq!(limited.iterations, 950);
        }
    }
}
