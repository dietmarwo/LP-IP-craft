# LP8: Two-stage planning under uncertainty

This executable chapter uses the existing native Rust solver through its Python API. The numerical definitions live with this chapter's owner; the shared driver supplies orchestration and figures.

## Model and decision

A newsvendor buys before demand is known. Unit purchase cost is 4, sale price 10 and salvage 1. Demand (20,40,60,80,100) has probabilities (0.3,0.3,0.2,0.1,0.1). A second application shares initial capacity across two products and buys costly recourse after demand is observed.

## Work through the lesson

Order $q$ units before demand is known. Purchase costs 4, sales earn 10 and leftovers recover 1. Demand $(20,40,60,80,100)$ has probabilities $(0.3,0.3,0.2,0.1,0.1)$.

1. Use the critical fractile $(10-4)/(10-1)=2/3$ to predict $q=60$, then solve the deterministic-equivalent LP. Expected profit is 198.
2. Evaluate the mean-demand decision $q=48$ in the original scenarios: it earns 190.8. The value of the stochastic solution is 7.2. Optimizing after observing each scenario earns 288, so perfect information is worth 90.
3. In `two_product`, first-stage orders share capacity 80; expensive emergency purchases supply recourse. Training samples use seed 42; a separate seed 314159 supplies the 128-case holdout. Study sizes 8, 16 and 32 describe sample sensitivity, not a statistical performance proof.

## Run

Install the pinned solver distributions and scientific dependencies using the [shared setup](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/structured/README.md). Keep the patched repositories as sibling directories. From this repository's root:

```bash
python ../optimization-jobs/tutorials/structured/run.py --chapter LP8 --output /absolute/new/lp8
```

For the owner entry point, run `python tutorials/stochastic-planning/run.py --preset smoke --output /absolute/new/lp8-direct`. It produces JSON; the shared driver additionally renders PNG/SVG figures. `--preset study` selects the declared larger cases. Output may be new or empty, and an occupied directory is refused.

## Independent answer and validation

The critical ratio is 2/3; order 60 yields expected profit 198. Ordering mean demand 48 yields 190.8. Perfect-information profit is 288, so VSS=7.2 and EVPI=90. The two-product model uses fixed training and disjoint holdout seeds; study runs use 8,16,32 scenarios.

Every run retains native status, points and available diagnostics. LP candidates are checked against row/bound/integrality requirements; SQP results retain derivative checks and independently reconstructed KKT residuals. Application-specific references are computed separately. All failures remain in the report.

## Read the result

Read `summary.json` before interpreting `figure.png`. The report records the actual result; expected reference values are not substituted for missing numerical outcomes. A completed command job means that the chapter's executable checks completed, while individual solver attempts can still be nonconvergent where the lesson explicitly retains them.

A sampled-scenario experiment is descriptive and retains its actual scenario set. Holdout data is not used to choose the training solution.

## Agent prompt

> Choose an order before demand is known, with recourse after observation. Compare the stochastic decision, the mean-demand decision and perfect information. Evaluate the two-product plan on its disjoint held-out scenarios.

Use the complete [prompt](prompt.md) for the workflow and evidence requirements.

## Follow-up exercise

Explain why the wait-and-see solution is not a realizable first-stage plan. Inspect holdout profit as the scenario sample changes.

## Provenance and limits

The original plan's probes are historical planning evidence. The checked-in `results/smoke/summary.json` and figures record this implementation's executed case. Source and installed-package identities are also recorded by the shared driver. These are reproducible teaching experiments, not industrial or hardware performance certifications. Cross-formulation and cross-solver tables are descriptive.

## Recorded example

![Recorded LP8 smoke result](results/smoke/figure.png)

Inspect the [complete smoke summary](results/smoke/summary.json) alongside the figure.

The [study summary](results/study/summary.json) and [study figure](results/study/figure.png) retain the declared study preset separately.
