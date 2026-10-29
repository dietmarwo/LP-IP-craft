# LP3: Sudoku: feasibility and uniqueness

This chapter runs native Rust solvers through their Python APIs. Its model and independent checks live in this directory; the shared driver in optimization-jobs adds figures and command-job execution.

## Model and decision

One binary per row, column and digit, with cell, row-digit, column-digit and box-digit equalities. Clues fix selected variables. A no-good row sums the solved values of free cells and excludes exactly one completed grid.

## Work through the lesson

A binary $x_{rcv}$ says that cell $(r,c)$ contains value $v$. Every cell, row/value, column/value and block/value group sums to one; clues fix selected binaries. The objective is zero: this is feasibility.

1. Inspect the 9×9 solution and check all four families directly. Black digits in the figure are clues; blue digits are filled cells.
2. Exclude the found grid with a no-good row over its free cells, $\sum_{(r,c)\ \text{free}}x_{r,c,v^\star_{rc}}\le|\text{free}|-1$, and solve again. Infeasibility of that model is the native uniqueness claim, with `claimed` evidence; an independent backtracking count supplies separate evidence.
3. Enumerate all 288 complete 4×4 Shidoku grids. Generated partial grids with one and two completions test the counter, the encoding and the no-good row: after the cut, the one-completion grid must be infeasible and the two-completion grid must still have a solution.
4. Compare the retained 9×9 unique and two-solution variants. A found completion alone says nothing about uniqueness.

## Run

Install the pinned solver packages and scientific dependencies as described in the [shared setup](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/structured/README.md), with the repositories it lists cloned side by side. From this repository's root:

```bash
python ../optimization-jobs/tutorials/structured/run.py --chapter LP3 --output /absolute/new/lp3
```

To run the chapter on its own, use `python tutorials/sudoku/run.py --preset smoke --output /absolute/new/lp3-direct`. It produces JSON; the shared driver additionally renders PNG/SVG figures. `--preset study` runs the larger declared cases; a chapter without them repeats the smoke case. Output may be new or empty, and an occupied directory is refused.

## Independent answer and validation

The independent counter enumerates 288 complete 4x4 grids, freezes a unique and a two-solution tiny puzzle, and counts the checked-in 9x9 case. Removing row 5 column 3 creates two solutions. The native solver finds the first and the no-good model tests the second.

Every run keeps the native status, points and available diagnostics, and every LP candidate is checked against its rows, bounds and integrality independently of the solver. Application-specific references are computed separately. All failures remain in the report.

## Read the result

Read `summary.json` before interpreting `figure.png`. The report records the actual result; expected reference values are not substituted for missing numerical outcomes. A completed command job means that the chapter's executable checks completed, while individual solver attempts can still be nonconvergent where the lesson explicitly retains them.

Uniqueness rests on the independent finite counter. Numerical infeasibility after the cut remains the solver's own claim. This is a feasibility model; a zero objective does not rank completed grids.

## Agent prompt

> Solve the supplied Sudoku and tell me whether it has exactly one completion. Explain the no-good cut, check it with a separate counter, and distinguish finding a solution from showing uniqueness.

Use the complete [prompt](prompt.md) for the workflow and evidence requirements.

## Follow-up exercise

Remove the designated clue. Deliberately use an all-cells no-good sum with a free-cell right-hand side and explain why it could fake uniqueness.

## Provenance and limits

The checked-in `results/` record one run of each preset. The shared driver writes source and package identities to `environment.json` for the runs it makes. These are reproducible teaching experiments, not industrial or hardware performance certifications. Tables that compare formulations or solvers are descriptive.

## Recorded example

![Recorded LP3 smoke result](results/smoke/figure.png)

Inspect the [complete smoke summary](results/smoke/summary.json) alongside the figure.

The [study summary](results/study/summary.json) and [study figure](results/study/figure.png) record the study preset.
