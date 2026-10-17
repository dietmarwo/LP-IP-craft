use lp_ip_craft::*;

pub fn fixture(name: &str) -> (Model, Option<f64>) {
    let (n, m, objective, offset, kinds, upper, row_rhs, rows, expected) = match name {
        "box-lp" => {
            let n = 100_000;
            (
                n,
                0,
                vec![1.; n],
                0.,
                vec![VariableKind::Continuous; n],
                vec![1.; n],
                vec![],
                vec![],
                Some(0.),
            )
        }
        "sparse-import-lp" => {
            let n = 40_000;
            let rows = (0..n)
                .map(|i| {
                    let mut row = (0..5).map(|k| ((i + k * 7919) % n, 1.)).collect::<Vec<_>>();
                    row.sort_by_key(|e| e.0);
                    row
                })
                .collect();
            (
                n,
                n,
                vec![1.; n],
                0.,
                vec![VariableKind::Continuous; n],
                vec![1.; n],
                vec![3.; n],
                rows,
                Some(0.),
            )
        }
        "transport-lp" => {
            let (origins, destinations) = (60, 80);
            let n = origins * destinations;
            let mut rows = vec![vec![]; origins + destinations];
            for j in 0..n {
                rows[j / destinations].push((j, 1.));
                rows[origins + j % destinations].push((j, 1.));
            }
            let objective = (0..n)
                .map(|j| (1 + (j * 37 + j / destinations * 11) % 101) as f64)
                .collect();
            let mut rhs = vec![destinations as f64; origins];
            rhs.extend(vec![origins as f64; destinations]);
            (
                n,
                origins + destinations,
                objective,
                0.,
                vec![VariableKind::Continuous; n],
                vec![f64::INFINITY; n],
                rhs,
                rows,
                Some(16980.),
            )
        }
        "partition-mip" => {
            let (groups, width) = (2000, 6);
            let n = groups * width;
            let objective = (0..groups)
                .flat_map(|i| (0..width).map(move |j| (1 + (i * 37 + j * 13) % 97) as f64))
                .collect::<Vec<_>>();
            let expected = 7.
                + objective
                    .chunks(width)
                    .map(|c| c.iter().copied().fold(f64::INFINITY, f64::min))
                    .sum::<f64>();
            let rows = (0..groups)
                .map(|i| (0..width).map(|j| (i * width + j, 1.)).collect())
                .collect();
            (
                n,
                groups,
                objective,
                7.,
                vec![VariableKind::Binary; n],
                vec![1.; n],
                vec![1.; groups],
                rows,
                Some(expected),
            )
        }
        _ => panic!("unknown import fixture {name}"),
    };
    let mut ptr = vec![0];
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for row in rows {
        for (j, v) in row {
            indices.push(j);
            values.push(v);
        }
        ptr.push(indices.len());
    }
    let equality = matches!(name, "transport-lp" | "partition-mip");
    let row_lower = if equality {
        row_rhs.clone()
    } else {
        vec![f64::NEG_INFINITY; m]
    };
    (
        Model::new(ModelData {
            sense: Sense::Minimize,
            objective,
            objective_offset: offset,
            variables: Bounds::new(vec![0.; n], upper),
            kinds,
            matrix: CsrMatrix::new(m, n, ptr, indices, values),
            rows: Bounds::new(row_lower, row_rhs),
        })
        .unwrap(),
        expected,
    )
}
