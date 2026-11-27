# Review of the Gurobi-gap delivery, revision 2

27 November 2026. The delivery is `LP-IP-craft-Gurobi-gap-v2-patch.zip`
(see [../EVIDENCE.md](../EVIDENCE.md)). Its full patch applied unchanged to
`bcc4af3` and gave the tree the archive records (`c289160`); that is the
first commit of #53. [RESULTS.md](RESULTS.md) is the delivered report and is
left as delivered; [PLAN.md](PLAN.md) is the plan it worked from. This page
records what the review measured, what it changed and what it left open.

**The delivered figures reproduce, and unchanged defaults do the same work
as 0.30.0.** The second statement is established here: the archive's
"fixed-work default comparison" compares the preceding patch with revision 2,
not the unpatched solver with revision 2. Both options stay off by default.
The review changed no search decision.

## What was measured

Three release builds were made here from source: the unpatched base
`bcc4af3` (0.30.0 plus the LocalMIP pilot), the preceding patch (revision 2
with the archive's `v1-to-v2` patch reversed) and revision 2. Gurobi 14.0.0
ran under its size-limited license. The host has two cores, so wall times
are noisier than in the delivery.

| Check | Delivered | Here |
| --- | --- | --- |
| Archive inventory | 171 entries | all match; the bundled report equals the one delivered separately |
| Rust tests, Python tests | 231, 409 | 231, 409 on the delivered tree (414 after this review) |
| Oracles with both options forced on | 600 exact, 549 of 550 SciPy | the same |
| Audit of the archived journals | 531 calls, 507 points, no discrepancy | recomputed from the archived models: the same |
| Scores from the archived journals | 0.726 and 0.765; 2.76 to 2.00 and 0.85 to 0.65 against Gurobi; 31.5% closure | reproduced by the delivered scorer and by an independent script |
| 35 cases rerun, three repetitions | 0.726 (0.566 to 0.888) | 0.727 (0.578 to 0.869); against Gurobi 2.79 to 2.02 |
| 24 fresh cases rerun, three repetitions | 0.765 (0.583 to 0.964) | 0.768 (0.616 to 0.912); against Gurobi 0.91 to 0.70 |
| Six public cases | native 3 of 6, Gurobi 5 of 6 | the same statuses and objectives |
| `noswot`, 150,000 nodes, both options | 122,185 nodes and `numerical_failure` before; 150,000 nodes and 864,337 pivots after | the same counts, one retry |
| Unpatched defaults against new defaults, fixed work | not in the archive | see below |

Solved counts in the rerun were 29, 30 and 34 of 35 for the preceding patch,
revision 2 and Gurobi. The extra native solve is `stein45`, which revision 2
finished in 9.0 to 9.5 s here and which sits at the ten-second cap.

The two regressions the report flags did not repeat: `gap-241` took 41.8 and
38.9 ms and `intcover-617` 11.4 and 11.1 ms. The fresh integer-covering
family took 9.09 and 8.77 ms, against 9.65 and 14.36 ms in the delivery.

## Findings

1. **The default comparison in the archive has the wrong baseline.**
   `delivery-default.jsonl` pairs the extension `739ebb19...` (the preceding
   patch) with `7468357e...` (revision 2); the unpatched extension is
   `7267ff4e...`. Rerun here with unpatched defaults against revision 2
   defaults on all 69 archived inputs: at 250 nodes and 10,000 pivots, 68
   pairs match in status, point, objective, bound, nodes and pivots, and
   `sp150x300d` differs only in its bound (35.13 to 36). At 20,000 nodes and
   400,000 pivots, 67 match and two differ only in the bound (`sp150x300d`
   37.89 to 38, `noswot` -43.00000000000016 to -43). Three timed repetitions
   of the 35 cases gave a ratio of 1.002 (0.994 to 1.010) with every solved
   pair identical in nodes and pivots. The report's sentence is true of the
   released solver; the archive did not show it.

2. **The gain over the released defaults is smaller than the headline.**
   0.726 is revision 2 against the preceding patch with both options on. In
   the archive the preceding patch's options were slower than unpatched
   defaults (score 0.2175 against 0.1889), so part of the gain recovers that
   loss. Against unpatched defaults the report gives 0.836 (0.655 to 1.027);
   two consecutive runs here gave 0.79. By family the gain is lot sizing
   (0.20), generalized assignment (0.53) and `mod008` (0.88). Integer
   covering is 1.15, and every other family is within 5%. The interval includes 1, which supports
   keeping the options off by default.

3. **The numerical failure on `noswot` is not a defect of 0.30.0.** It
   occurred with the preceding patch's options on. Unpatched defaults reach
   150,000 nodes there with the same nodes and pivots as revision 2
   defaults. In every run made here the pricing retry fired once, in that
   run.

4. **A limited run can now hold a proof and still report its limit.** In a
   mixed model with an integral objective only the returned bound is
   rounded. When the rounded bound meets the incumbent, the status stays
   `node_limit`. In 36,384 solves of random mixed models, most of them at
   node limits from 0 to 17, 2,302 ended that way, against 187 on the
   unpatched build. No
   bound exceeded the optimum of HiGHS. [docs/api.md](../api.md) now says so
   and a test pins it.

5. **The equality option can change a status on decimal data.** The default
   presolve reads the stored doubles exactly. `15x + 0.1y = 29.8` over
   bounded integers has no integer point as stored, and the answer is
   `infeasible`. With `integer_equality_presolve` the row becomes
   `150x + y = 298`, the point (2, -2) is kept, the result check accepts it
   and the answer is `optimal`. The exact reading is this project's stated
   policy ([safe-dual-bounds.md](../safe-dual-bounds.md), and two tests that
   name it), so the review documents the difference and does not change the
   presolve. See the first open point.

6. **What the equality option buys was not stated.** On the six archived
   equality chains, defaults stop at the ten-second cap after about a
   million nodes each. With the option each chain solves in at most 0.36 s
   (156 to 28,985 nodes). Gurobi solves each at the root.

## Changes

1. **The scorer needed a Gurobi arm.** The README calls Gurobi optional, but
   `summarize.py` rejected a journal without `gurobi:default`.
   `--no-reference` scores the native arms alone. The scorer had tests for
   its rejections only; a new test checks the score, the paired ratio, the
   Gurobi ratios and the closure against values computed by hand.
2. **CI did not run the new checks.** The oracle job now runs both options
   against the exact and the SciPy oracle and runs the harness's tests.
3. **Tests.** 300 random integer equality systems are compared with
   enumeration under the equality option, alone and with the adaptive root.
   Two more tests pin the decimal example of finding 5 and the limited run
   of finding 4.
4. **Documentation.** [docs/api.md](../api.md) gains the two paragraphs for
   findings 4 and 5. The benchmark README described a patch still to be
   applied and now describes two builds. The changelog entries were
   rewritten.
5. A test for `settings.mip_presolve` inside a block that already requires
   it was removed.

## Not verified

- The full SpOC4 tables. Their data is not in the archive.
- The 60-second `noswot` run and the nine-pair replays of `gap-241` and
  `intcover-617`.
- The selection of the six public cases from Data-Miplib3. The archived
  models were rerun; the freeze from the repository was not repeated.
- Timings on the delivery's host. The reruns confirm ratios, not times.

## Open points

1. **Decimal data.** With decimal coefficients, 15 of 9,000 random small
   integer programs got `infeasible` or a worse objective from the default
   presolve although a point within the feasibility tolerance exists. With
   a right-hand side perturbed in the last place it was 1,441 of 9,000.
   0.30.0 behaves the same. Rounding integer bounds, integer row limits and
   the objective bound with the tolerance removes all of these and leaves
   nodes and pivots on the 69 fixtures unchanged. It also reverses the
   policy that two Python tests and one Rust test pin, so it is offered
   as a separate pull request for a decision.
2. **Use the rounded bound in the mixed tree.** Pruning by the rounded bound
   while keeping node order would close the runs of finding 4 as `optimal`
   and save nodes. The report notes that rounding the priorities lost an
   incumbent on `noswot`; pruning alone was not tried.
3. **Default promotion.** It needs a fresh cohort and an interval against
   unpatched defaults that excludes 1.
4. `vpm1`, `vpm2`, `gesa2_o` and, at the cap, `stein45` still time out.
   `pp08a`, `set1ch`, `pp08aCUTS` and `danoint` are still declined for
   infinite bounds.
