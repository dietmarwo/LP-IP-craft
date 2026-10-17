//! cargo run --release -p lp-ip-craft-native --features kernel-profile
//! --example spoc4_profile -- path/to/matching-i.txt 5 [packing]
use lp_ip_craft::*;
use lp_ip_craft_native::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let text = std::fs::read_to_string(args.get(1).ok_or("matching table path required")?)?;
    let mut transfers = Vec::new();
    let mut keys = BTreeSet::new();
    let mut weights = Vec::new();
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 4 {
            return Err("four fields required".into());
        }
        let mut t = [0u64; 3];
        for k in 0..3 {
            t[k] = fields[k].parse()?;
            keys.insert((k, t[k]));
        }
        let (whole, fraction) = fields[3].split_once('.').unwrap_or((fields[3], ""));
        if fraction.len() > 3 {
            return Err("at most three decimal digits required".into());
        }
        weights.push(
            (whole.parse::<u64>()? * 1000 + format!("{fraction:0<3}").parse::<u64>()?) as f64,
        );
        transfers.push(t);
    }
    let map: BTreeMap<_, _> = keys
        .into_iter()
        .enumerate()
        .map(|(i, key)| (key, i))
        .collect();
    let mut rows = vec![Vec::new(); map.len()];
    for (j, t) in transfers.iter().enumerate() {
        for k in 0..3 {
            rows[map[&(k, t[k])]].push(j);
        }
    }
    rows.retain(|r| r.len() > 1);
    let n = weights.len();
    let m = rows.len();
    let mut indptr = vec![0];
    let mut indices = Vec::new();
    for row in rows {
        indices.extend(row);
        indptr.push(indices.len());
    }
    let values = vec![1.; indices.len()];
    let model = Model::new(ModelData {
        objective: weights,
        objective_offset: 0.,
        sense: Sense::Maximize,
        variables: Bounds::new(vec![0.; n], vec![1.; n]),
        kinds: vec![VariableKind::Binary; n],
        matrix: CsrMatrix {
            pattern: CsrPattern {
                rows: m,
                cols: n,
                indptr,
                indices,
            },
            values,
        },
        rows: Bounds::new(vec![f64::NEG_INFINITY; m], vec![1.; m]),
    })?;
    let seconds = args.get(2).map(|v| v.parse()).transpose()?.unwrap_or(5.);
    let packing = args.get(3).is_some_and(|s| s == "packing");
    #[cfg(feature = "kernel-profile")]
    profile::reset();
    let start = Instant::now();
    let r = NativeSolver::new(NativeOptions {
        time_limit_seconds: Some(seconds),
        component_threads: 1,
        packing_root: packing,
        ..Default::default()
    })
    .solve(
        &model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    )?;
    println!(
        "{{\"status\":{:?},\"seconds\":{},\"pivots\":{},\"profile\":[",
        r.status.as_str(),
        start.elapsed().as_secs_f64(),
        r.iterations
    );
    #[cfg(feature = "kernel-profile")]
    for (index, (name, ns, count)) in profile::snapshot().iter().enumerate() {
        if index > 0 {
            println!(",");
        }
        print!("{{\"operation\":{name:?},\"exclusive_nanoseconds\":{ns},\"count\":{count}}}");
    }
    println!("]}}");
    Ok(())
}
