# Native 0.26 results

All five priorities in PLAN.md are implemented in native Rust and exposed through Rust/Python controls. Defaults preserve the previous algorithm choices. Validation passes with **160 Rust tests and 243 Python tests**, including independent exhaustive integer oracles. The original two matching tables demonstrate better feasible incumbents; neither full integer optimum is proved.

## Application outcomes

The strongest tested fixed repair schedule (eight rounds of 64 rejected-transfer neighborhoods, four workers, compatible merge enabled) returns:

- matching-I: **32,847.565**, versus **32,120.012** from the 0.25 policy, an improvement of **727.553 / 2.27%**. Actual solve time is **1.614 seconds**.
- matching-II: **70,945.093**, versus **68,914.100**, an improvement of **2,030.993 / 2.95%**. Actual solve time is **6.431 seconds**.

Both are independently checked feasible selections in original transfer order and original resource namespaces. They are heuristic incumbent scores. The bound, integer optimum and official competition validation are separate matters.

Every main policy starts from the same deterministic weight-greedy vector with zero-weight columns excluded. This reproduces the native 0.25 seed: the fresh legacy policy reproduces both published scores, and a cold matching-I check returns the same vector/work. The tables contain two/six zero-weight columns in I/II. The exploratory adapter start retained some zero-weight selections and gave slightly different later trajectories; those results are preserved separately under `results-adapter` and are not mixed into the table below.

All runs have the same 15-second whole-call cap, but proof-disabled fixed-work repairs finish much earlier. Times are actual solver-call wall times; input/model construction is separate. These are single shared-host measurements, not statistically stable speed ratios. Increasing task count is additional work, not a speedup claim.

| Policy | Planned repairs | I score | I seconds | II score | II seconds |
|---|---:|---:|---:|---:|---:|
| Legacy best-standalone merge | 64 | 32,120.012 | 0.435 | 68,914.100 | 1.191 |
| Compatible merge | 64 | 32,349.711 | 0.442 | 69,326.273 | 1.210 |
| Compatible merge, four rounds, incumbent seeds | 256 | 32,705.745 | 0.899 | 70,066.954 | 3.327 |
| Four rounds, rejected seeds | 256 | 32,775.974 | 0.928 | 70,392.990 | 3.536 |
| Four rounds, mixed seeds | 256 | 32,701.772 | 0.867 | 70,157.806 | 3.256 |
| Mixed seeds + local clique cuts | 256 | 32,701.772 | 0.930 | 70,157.806 | 3.594 |
| Mixed seeds + growing neighborhoods | 256 | 32,780.045 | 1.035 | 70,322.067 | 3.630 |
| Rejected seeds, eight rounds | 512 | 32,847.565 | 1.614 | 70,945.093 | 6.431 |

Four rounds with the same residual capacity/solver/budgets allow a direct comparison of seed strategies. Rejected seeds win on these two pilots; mixed is not universally better than incumbent-centered selection. Growing neighborhoods helps the mixed policy here, but uses more local LP work. Partial trajectories and exact effective settings are saved in every result.

## 1. Compatible repair combination

The exact six-transfer fixture checks a snapshot score of 12, two standalone repair scores of 16, and a compatible union of 20. Shared-change and cross-resource collisions are rejected atomically, and the best standalone fallback survives. The same fixture checks one/four/sixteen-worker equality.

On both full tables, the 64-task merge mode improves on the common-start legacy mode with slightly fewer residual pivots (I: 2,006→1,923; II: 2,108→1,977). These pilot differences are evidence for incumbent quality, not an assertion that merge always reduces solve effort. The greedy delta order can reject useful overlapping combinations; it does not solve the optimal repair-combination problem.

## 2. Successive rounds

Eight-round rejected schedules continue improving beyond round four. I moves from 32,299.132 after round one through 32,775.974 after round four to 32,847.565 after round eight. Complete trajectories include cumulative tasks, nodes/pivots, effective column capacity and repair elapsed time.

All rounds share the parent node/pivot budget. Some tasks safely produce no residual when their round-ranked seed has become selected in a later batch. The I/II eight-round runs use 510/521 residual nodes respectively, versus 512 planned tasks; extra nodes are possible when a residual branches, and stale tasks can use none. No local errors occur.

## 3. Selection and parallelism

Rejected seeds explicitly release every movable incumbent blocker and retain the seed before truncation. Fixed-one blockers exclude a seed. Exhaustive boundary fixtures and random integer-oracle tests check that every accepted residual solution reconstructs to a feasible original selection. Root prices affect ranking only.

Both full-table rejected schedules have exactly the same original-order vector, objective, nodes and pivots with one/four workers. Actual whole-call times are 1.034/0.928 seconds for I and 4.117/3.536 seconds for II (about 1.11× / 1.16× in these observations). Ordered validation/merging, root work and model setup remain serial. Sixteen-worker correctness is tested on fixed-work fixtures; the host's eight-core CPU quota does not support a sixteen-physical-core speedup claim.

## 4. Residual clique cuts

The cross-resource four-transfer fixture has LP value 2, all four triangle cuts give 4/3, and the new four-clique row gives 1. Exhaustive random tests preserve every feasible binary point. A separate Python test confirms that these cuts execute inside residual repairs with no global triangle pass or proof nodes.

On the full-table mixed schedules, local separation adds 5/10 clique rows across all tasks and consumes 1,065,773/1,570,183 bounded separator operations. Scores and local pivot totals remain unchanged. Actual elapsed time increases from 0.867→0.930 seconds and 3.256→3.594 seconds. The feature is implemented and correct but has no demonstrated application benefit in this pilot; leave it off for the supplied quality recipe.

## 5. Root and proof budgets

Adding a two-second exact proof phase to the mixed repair schedule leaves score and safe bound unchanged. I processes one large root and 9,145 proof pivots in 2.005 seconds; II processes one root and 4,865 pivots in 2.028 seconds. Neither root completes. These measurements justify continuing repair work separately from proof work; a longer global simplex run should not be presented as useful incumbent search without evidence.

Proof-disabled runs return `time_limit` because the explicit proof phase cap is zero, with zero proof nodes/pivots. They retain their checked incumbent and safe root-price bound. Setup of that zero-work result still takes measurable time. Limits are cooperative; actual elapsed phase times are recorded.

The main root-price upper bounds remain 34,153.324 and 76,765.388. Matching-I's independently reverified plain-LP price bound is 34,120.529, only 32.795 tighter than its native price bound. That small additional plain-LP tightening does not address the much larger distance from the incumbent to the relaxation bound.

For matching-II, raising root work from 200 million to one billion units (2,000-step ceiling) with the same mixed repair schedule improves the checked upper bound to **75,498.905** and the incumbent from **70,157.806 to 70,715.443**, in **5.067 seconds** instead of 3.256. More root guidance has measurable value on this instance. A root-work proxy is not an instruction-count limit, and this is not a new exact LP implementation.

An independent HiGHS 1.15.1 IPM reference for matching-II stops at 60 seconds without an available solution/price vector. Extending the cap to 120 seconds completes in **83.335 seconds**, with numerical LP objective **75,359.998751** (27 IPM iterations, no crossover). Saved dyadic nonnegative resource prices independently establish the exact integer-objective upper bound **75,359.999**, using numerator 79,020,686,429,509 and denominator 1,048,576 plus the objective lattice. The audit recomputes this using integer arithmetic and the original matrix. This is a verified upper bound, not an exact integer optimum or a native Rust solver dependency.

Thus matching-II's native 200-million-work bound has 1,405.389 of room to that reference bound, whereas the one-billion-work bound has only 138.906. This supports allocating more root work specifically to II while keeping repair and proof budgets distinct. Both full integer optimum values remain unknown.

## Validation and limits

- Rust workspace/default and numerical all-targets/all-features suites: 160 tests each, zero failures.
- Python: 243 tests, zero failures; 240 new small MIP solves use independent exhaustive integer optima across three new-mode combinations, in addition to the previous 400-mode oracle coverage.
- Local clique validity additionally checks every feasible assignment on 80 random eight-variable models. Existing 120 random bound/cut fixtures remain green.
- Formatting and Clippy pass with default/all features and warnings denied.
- Targeted tests cover deterministic merge/fallback, later-round gains, seed/fixed-bound reconstruction, local clique LP strength, zero/tiny work and phase caps, shared global node/pivot limits, original-row price diagnostics and cancellation retaining a checked start.
- Original-resource audit rechecks every saved solution vector, exact integer score, round monotonicity, work reservations and available bound. **38 saved vectors pass with zero failures**, including all primary and exploratory policies; exact reference bounds and one/four-worker equality are also audited in `benchmarks/spoc4-026/AUDIT.json`.

The official ESA validator and historical Gurobi formulation/configuration remain unverified. Nonnegative weights and the actual table dimensions are inspected, not assumed. No new Gurobi performance comparison is claimed. Neither local repairs, numerical LP references nor conservative dual bounds prove either full integer optimum.
