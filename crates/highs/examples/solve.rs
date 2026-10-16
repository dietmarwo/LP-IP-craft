//! Solves the production-planning LP of the project's LP/IP tutorial
//! (Example 1) with HiGHS and re-checks the solution and duals independently:
//! `cargo run -p lp-ip-craft-highs --example solve`.
//!
//! ```text
//! maximize 45 chair + 80 table + 110 desk
//! s.t.      5 chair + 20 table + 25 desk <= 400   (wood)
//!          10 chair + 15 table + 20 desk <= 450   (labor)
//!             chair +  3 table +  5 desk <=  60   (finishing)
//!          x >= 0
//! ```

use lp_ip_craft::check::lp_residuals;
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, LpIpSolver, Model, ModelData, Options, Result, Sense,
    Status, VariableKind,
};
use lp_ip_craft_highs::HighsSolver;

fn main() -> Result<()> {
    let model = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![45.0, 80.0, 110.0],
        objective_offset: 0.0,
        variables: Bounds::non_negative(3),
        kinds: vec![VariableKind::Continuous; 3],
        matrix: CsrMatrix::from_dense(3, 3, &[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0])?,
        rows: Bounds::new(vec![f64::NEG_INFINITY; 3], vec![400.0, 450.0, 60.0]),
    })?;

    let result = HighsSolver::default().solve(
        &model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    )?;
    println!("status: {:?} ({})", result.status, result.message);
    println!("backend: {:?}", result.backend);
    assert_eq!(result.status, Status::Optimal);

    let x = result.candidate.as_ref().expect("an optimal candidate");
    let evaluation = result
        .evaluation
        .as_ref()
        .expect("evaluated with the candidate");
    println!("x = {x:?}, profit {}", evaluation.objective);
    assert!((evaluation.objective - 2150.0).abs() < 1e-8);

    // HiGHS's duals in the crate's convention c = A^T y + z; the tutorial
    // reports row duals (0, 11/3, 25/3) and reduced costs (0, 0, -5).
    let duals = result.duals.as_ref().expect("LP duals");
    println!("duals: {duals:?}");
    let residuals = lp_residuals(&model, x, duals)?;
    println!("independent LP residuals: {residuals:?}");
    assert!(residuals.max() < 1e-9);
    Ok(())
}
