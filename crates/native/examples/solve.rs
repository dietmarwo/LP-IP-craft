use lp_ip_craft::*;
use lp_ip_craft_native::*;
fn main() -> Result<()> {
    let model = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![7., 11., 5., 6.],
        objective_offset: 0.,
        variables: Bounds::new(vec![0.; 4], vec![1.; 4]),
        kinds: vec![VariableKind::Binary; 4],
        matrix: CsrMatrix::from_dense(1, 4, &[3., 5., 2., 4.])?,
        rows: Bounds::new(vec![f64::NEG_INFINITY], vec![8.5]),
    })?;
    let r = NativeSolver::default().solve(
        &model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    )?;
    assert_eq!(r.status, Status::Optimal);
    println!("{r:?}");
    Ok(())
}
