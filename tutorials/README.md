# Structured application tutorials

Each chapter is a Python script that builds a model, solves it with the native solver (`backend="native"`) and checks the answer against an independent reference. Run one with `python tutorials/<chapter>/run.py --output /new/dir`. Start with the first two chapters, then pick an application.

| ID | Chapter |
|---|---|
| LP1 | [Resource allocation: relaxation to integer decisions](resource-allocation/README.md) |
| LP2 | [Battery dispatch: time coupling and operating modes](battery-dispatch/README.md) |
| LP3 | [Sudoku: feasibility and uniqueness](sudoku/README.md) |
| LP4 | [Staff rostering and an exact lower bound](staff-rostering/README.md) |
| LP5 | [Transportation, integrality and marginal prices](transportation-duals/README.md) |
| LP6 | [Facility location: formulation strength](facility-location/README.md) |
| LP7 | [Cutting stock and column generation](cutting-stock/README.md) |
| LP8 | [Two-stage planning under uncertainty](stochastic-planning/README.md) |
| LP9 | [SpOC4 matching: incumbent quality at a budget](spoc4-matching/README.md) |

[LP10: rail formulation](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/rail-formulation/README.md).

The [shared setup](https://github.com/dietmarwo/optimization-jobs/blob/main/tutorials/structured/README.md) in optimization-jobs runs the whole twenty-chapter series (LP, SQP, fcmaes and cross-solver chapters) directly or as trusted command jobs. The [job API tutorial](job-api/README.md) shows the `lp-ip-job-worker` profiles.
