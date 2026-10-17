use lp_ip_craft_native::{profile, replay};
use serde_json::json;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let path = std::path::Path::new(args.get(1).expect("basis file"));
    let repeats: usize = args.get(2).map_or(3, |s| s.parse().unwrap());
    let case = replay::BasisCase::read(path).unwrap();
    let mut runs = Vec::new();
    for run in 0..=repeats {
        profile::reset();
        let result = replay::run(&case, 20_000_000).unwrap();
        runs.push(json!({"run":run,"factor_seconds":result.factor_seconds,"solve_seconds":result.solve_seconds,
            "entries":result.entries,"dense_dimension":result.dense_dimension,"pivot_fingerprint":format!("{:016x}",result.pivot_fingerprint),
            "backward_error":result.max_backward_error,"accepted":result.max_backward_error < 1e-9,"kernel_profile":profile::snapshot()}));
    }
    println!("{}",json!({"file":path.file_name().unwrap().to_str(),"m":case.columns.len(),"nnz":case.columns.iter().map(Vec::len).sum::<usize>(),"rhs":case.rhs.len(),"runs":runs}));
}
