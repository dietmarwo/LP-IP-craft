# Review of the 30-iteration performance delivery

27 November 2026. The delivery is
`LP-IP-craft-30-iteration-performance-patch.zip` (see
[../EVIDENCE.md](../EVIDENCE.md)). Its patch applied unchanged to `2d6c431`
(tag `v0.31.0`) and gave the tree the archive records (`129566e`); that is
the first commit of the pull request. [RESULTS.md](RESULTS.md) is the
delivered report, left as delivered; [PLAN.md](PLAN.md) is the plan it
worked from. This page records what the review measured, what it changed
and what it left open.

**The four retained changes are sound and the delivered figures reproduce,
with one exception in the delivery's disfavour that did not reproduce: the
large packing LP is not slower here.** The review found no wrong answer. It
added the unit tests the patch lacked and removed one obstacle in the new
option: a start that failed the exact check blocked admission.

## What was measured

Release builds of 0.31.0 and of the patched tree were made here from
source. The host has two cores, so wall times are noisier than in the
delivery. HiGHS, through SciPy, served as the reference solver.

| Check | Delivered | Here |
| --- | --- | --- |
| Archive inventory | 644 entries | all match; the bundled plan and report equal the ones delivered separately |
| Rust tests | 234 | 231 with default features (the other three need the `parallel-bench` and `basis-replay` features); 235 after this review |
| Python tests with the worker, harness tests | 409 + 9 | the same; 410 + 9 after this review |
| Oracles | 600 exact per profile; SciPy 549 of 550 compared | the same, no failure |
| Structured tutorials | not reported | all 16 recorded summaries reproduce |
| Fixed work, 0.31.0 against the patch, defaults | 94 of 96 identical at 250 nodes and 10,000 pivots | 94 of 96 at 1,000 nodes and 100,000 pivots, and the same with `adaptive_root` and `integer_equality_presolve` on; the 33 confirmation cases are identical |
| The two differences | reversed 128-step chains: `Unsupported` to optimal −1.75, 1 node, 255 pivots | the same |
| 1,024-column probe chain | 419.1 to 165.7 ms of preprocessing | 380 to 155 ms for the whole solve |
| `pp08a`, `set1ch` with the option | incumbents 8,020 and 107,691.5 | the same after 30 s, with bounds 5,277.7 and 39,032.4 |
| `pp08aCUTS`, `danoint` with the option | 7,700 with `numerical_failure`; `Unsupported` | the same (failure after 88,048 nodes) |
| `packing:4096` LP | 1.091 [1.066, 1.480], replay 1.190 | 0.998 [0.968, 1.002] over nine pairs, 4,098 pivots in both builds |

The patch does not touch the LP path: an LP returns before the changed
code. The delivery reports equal pivots, refactorizations and updates for
the slower runs, so its ratio measured code placement or the host, not an
algorithmic cost. `packing:1024` (0.996) and `network:2048` (0.993) were
also level here.

### Models with missing bounds against a reference solver

The archive checks the option on 400 enumerated models. The review used
other generators and HiGHS.

- 3,000 mixed models of two to seven columns, with about 45% of the
  non-binary sides missing, rows in both senses, equalities, ranges and
  fractional coefficients, both objective senses and offsets. 0.31.0 and
  the patch return the same status on every model with default options
  (2,369 optimal, 9 unbounded, 622 `Unsupported`), with or without a start.
  Two optimal results differ in the last digits, for example a bound of
  −16.999999999999986 against −16.99999999999999: the queue reaches the
  same inferences in another order, and outward rounding then differs.
- With `objective_cutoff_bounds` and the generator's feasible point as the
  start, 258 more of these models are solved (2,627). Every optimum agrees
  with HiGHS to a relative 1e-6. Without a start the option changes
  nothing on this set, because few of its models have the fixed-setup form.
- 3,000 fixed-setup models (continuous columns without an upper bound, a
  setup binary each, covering rows, both senses). Defaults solve 95, the
  option 1,189. All agree with HiGHS; runs limited to three pivots or one
  node return incumbents no better than the optimum and bounds that do not
  cut it. On 222 of the models the review ran every pivot limit from 1 to
  12: no run exceeded its limit, so the starter LP's pivots count.

### The visit limit of the queue

The queue visits at most four times as many rows as the model has. A build
with that limit raised a thousandfold solved no further model of the first
1,500 above; five optimal values moved in the last digits. A ladder that
needs two waves against the row order (x waits for y, y for its own chain)
resolves within the limit at 8, 32 and 128 steps in three row orders.

## Findings and changes

1. **None of the new routines had a Rust test.** The patch added three
   Python tests of the option. The one existing probing test uses six
   columns, so its trails never reach the 32 entries at which the stamped
   index takes over. Four unit tests were added:
   - long and short probe trails give the same bounds, with a second
     binary whose trail covers one side only, so that a stale entry of the
     first probe would be read if the stamp were ignored (a build with the
     stamp check disabled fails this test);
   - a 96-step chain against the row order and the two-wave ladder are
     inferred and solved, with exact expected bounds;
   - a cancelled or timed-out inference leaves the bounds unchanged or
     valid;
   - the objective caps of a continuous and an integer column, in both
     senses and with an offset: outward rounding, the floor of the integer
     cap, no cap when a column can improve without limit, and none when
     the supplied value is not attainable.
2. **A start that failed the exact check made the model `Unsupported`,
   while the same call without a start was solved.** The fixed-setup LP ran
   only when no start was given. A caller with a heuristic point that is
   feasible to the tolerances, or with a poor guess, got less than a caller
   with nothing. The LP now also runs when the supplied start fails the
   check. Caps still come only from a point that passes it. The caller's
   point remains the start while it lies in the capped model; otherwise
   the LP's point replaces it and `start_status` is `rejected`. The change
   only turns `Unsupported` into a solve under the option: the 3,000 mixed
   models give identical records before and after it, and on the
   fixed-setup models a zero start, a start at the finite bounds and no
   start now admit the same models.
3. **The changelog entry was silent on two visible effects.** Models can
   leave `Unsupported` by default, and values on models with missing bounds
   can move in the last digits. Both are now stated, as is the probe index.

## Not verified here

- The timed comparisons with Gurobi and the score ratios of the 35 blind
  cases (the fixed-work results are identical, so only timing is open).
- The archive's 800-solve enumeration oracle of the option and its
  `noswot` and SpOC4 records.
- Any of the 26 discarded trials.

## Open points

- `pp08aCUTS` ends in `numerical_failure` with the option. The status comes
  from the search on the capped model, not from the caps; the incumbent
  7,700 is checked. The caps of the fixed-setup incumbent are loose (every
  setup on), which gives a weak relaxation on these models.
- The option stays off by default and the fixed-setup form is narrow.
  Whether the starter should cover negative lower bounds or general
  integer setups is a design question for a later change.
