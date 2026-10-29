# LP6: Facility location: formulation strength

This executable chapter uses the existing native Rust solver through its Python API. The numerical definitions live with this chapter's owner; the shared driver supplies orchestration and figures.

## Model and decision

Five sites and ten weighted customers use fixed opening costs plus Manhattan service costs. Compare aggregated linking sum_j x_ij <= 10 y_i with the disaggregated x_ij <= y_i. Both express the same integer choices.

## Work through the lesson

Open-site binaries $y_i$ and assignment fractions $x_{ij}$ satisfy $\sum_i x_{ij}=1$. Weighted Manhattan distance gives service costs; the fixed-opening costs and coordinates are frozen in the canonical model.

1. Compare the five aggregated links $\sum_jx_{ij}\le10y_i$ with the 50 disaggregated links $x_{ij}\le y_i$. For binary $y$ they describe the same feasible assignments. Fractional openings exploit the aggregated version.
2. Read native work and bounds, then independently enumerate all 31 nonempty sets of open sites. Both MIPs cost 134 and open sites 4 and 5; the weak LP gives 87.6 and the strong LP 134.
3. Add the declared capacities. Enumeration now solves a small transportation LP per open set because demand can split. The capacitated optimum is 143; strong links do not generally imply an integral relaxation.

## Run

Install the pinned solver distributions and scientific dependencies using the [shared setup](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/structured/README.md). Keep the patched repositories as sibling directories. From this repository's root:

```bash
python ../optimization-jobs/tutorials/structured/run.py --chapter LP6 --output /absolute/new/lp6
```

For the owner entry point, run `python tutorials/facility-location/run.py --preset smoke --output /absolute/new/lp6-direct`. It produces JSON; the shared driver additionally renders PNG/SVG figures. `--preset study` selects the declared larger cases. Output may be new or empty, and an occupied directory is refused.

## Independent answer and validation

Enumerating 31 nonempty open sets gives integer cost 134. The weak relaxation is 87.6, while the strong one is 134 for this instance. A capacitated follow-up enumerates open sets with an independent transport LP and checks the native MIP.

Every run retains native status, points and available diagnostics. LP candidates are checked against row/bound/integrality requirements; SQP results retain derivative checks and independently reconstructed KKT residuals. Application-specific references are computed separately. All failures remain in the report.

## Read the result

Read `summary.json` before interpreting `figure.png`. The report records the actual result; expected reference values are not substituted for missing numerical outcomes. A completed command job means that the chapter's executable checks completed, while individual solver attempts can still be nonconvergent where the lesson explicitly retains them.

Formulation comparisons are descriptive because the input models differ. A node-count difference on this fixture is not a general timing result.

## Agent prompt

> Choose which candidate depots to open. Solve the aggregated and disaggregated formulations, explain why their relaxed bounds differ, and verify their common integer answer by open-set enumeration. Check the capacitated variant separately.

Use the complete [prompt](prompt.md) for the workflow and evidence requirements.

## Follow-up exercise

Drop one linking row and construct a closed-site assignment that the domain checker should reject. Explain why the strong relaxation need not be integral after capacity constraints.

## Provenance and limits

The original plan's probes are historical planning evidence. The checked-in `results/smoke/summary.json` and figures record this implementation's executed case. Source and installed-package identities are also recorded by the shared driver. These are reproducible teaching experiments, not industrial or hardware performance certifications. Cross-formulation and cross-solver tables are descriptive.

## Recorded example

![Recorded LP6 smoke result](results/smoke/figure.png)

Inspect the [complete smoke summary](results/smoke/summary.json) alongside the figure.

The [study summary](results/study/summary.json) and [study figure](results/study/figure.png) retain the declared study preset separately.
