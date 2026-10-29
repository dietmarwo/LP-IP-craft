# LP9: SpOC4 matching: incumbent quality at a budget

This chapter runs native Rust solvers through their Python APIs. Its model and independent checks live in this directory; the shared driver in optimization-jobs adds figures and command-job execution.

## Model and decision

Reuse the existing ESA matching parser, original resource identifiers, greedy seed and native set-packing adapter. Smoke uses the complete 25,000-edge matching-I table; study uses matching-II. Data stays in its original GPL-3.0 directory.

## Work through the lesson

The smoke preset uses all 25,000 matching-I edges from the existing competition fixture. Each selected edge consumes one resource from each of three partitions. The integer objective is published weight, with exact thousandths retained for auditing.

1. Inspect the input checksum and existing data-source/license record. The chapter reuses the maintained benchmark adapter and checker; it does not copy the competition data into a new dataset.
2. Compare the declared one- and three-second smoke limits. Both native runs end at their time limit after one node and return the greedy start unchanged: the incumbent is 29,791.692, `improved_on_start` is false and no native bound is returned (those fields remain null). At these budgets the search does not get past the root; the lesson is how to read such a result, not that the incumbent is good. The check requires the native and independently checked objectives to agree and to stay below the independent bound.
3. A separately checked dyadic packing certificate gives upper bound 41,661.853 on matching-I. Label it as independent evidence, not a bound returned by the native search. The gap is intentionally substantial.
4. Study selects matching-II with a larger budget. This is the SpOC matching application; it does not fill the ecosystem's GTOC target card.

## Run

Install the pinned solver packages and scientific dependencies as described in the [shared setup](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/structured/README.md), with the repositories it lists cloned side by side. From this repository's root:

```bash
python ../optimization-jobs/tutorials/structured/run.py --chapter LP9 --output /absolute/new/lp9
```

To run the chapter on its own, use `python tutorials/spoc4-matching/run.py --preset smoke --output /absolute/new/lp9-direct`. It produces JSON; the shared driver additionally renders PNG/SVG figures. `--preset study` runs the larger declared cases; a chapter without them repeats the smoke case. Output may be new or empty, and an occupied directory is refused.

## Independent answer and validation

Each candidate is checked against original resource triples and integer thousandths of the weight. Runs retain time/node/pivot information. When the native bound is unavailable, an independent dyadic packing bound is computed using the existing certificate helper.

Every run keeps the native status, points and available diagnostics, and every LP candidate is checked against its rows, bounds and integrality independently of the solver. Application-specific references are computed separately. All failures remain in the report.

## Read the result

Read `summary.json` before interpreting `figure.png`. The report records the actual result; expected reference values are not substituted for missing numerical outcomes. A completed command job means that the chapter's executable checks completed, while individual solver attempts can still be nonconvergent where the lesson explicitly retains them.

A time limit with a feasible incumbent is useful evidence. An unavailable native bound remains null. These tables do not fill the separate GTOC target card.

## Agent prompt

> Run the full SpOC4 matching fixture under its declared work budgets. Audit the incumbent, explain what missing native bounds mean, and distinguish an independent resource-packing bound from a native search claim.

Use the complete [prompt](prompt.md) for the workflow and evidence requirements.

## Follow-up exercise

Increase the declared budget and compare retained incumbents. Convert an MPS model using the existing converter, keeping that exercise distinct from the matching formulation.

## Provenance and limits

The checked-in `results/` record one run of each preset. The shared driver writes source and package identities to `environment.json` for the runs it makes. These are reproducible teaching experiments, not industrial or hardware performance certifications. Tables that compare formulations or solvers are descriptive.

## Recorded example

![Recorded LP9 smoke result](results/smoke/figure.png)

Inspect the [complete smoke summary](results/smoke/summary.json) alongside the figure.

The [study summary](results/study/summary.json) and [study figure](results/study/figure.png) record the study preset.
