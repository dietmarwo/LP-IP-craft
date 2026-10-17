# Review of the 0.24.0 delivery

The 0.24.0 archive was imported unchanged (apart from its raw `data/`
directories and stale packaging files, see [../EVIDENCE.md](../EVIDENCE.md)),
then reviewed. [RESULTS.md](RESULTS.md) describes the delivered build and is
left as delivered, so it still regenerates byte for byte from the frozen
data. This page records what the review changed and what it measured.

## Changes

1. **CI failed on the delivered source.** CI runs
   `cargo clippy --workspace --all-targets -- -D warnings` with default
   features; the delivered `final-clippy.log` was produced with
   `--all-features` only. Without `kernel-profile`, the no-op profiling
   guard tripped `clippy::drop_non_drop`. The guard now implements a no-op
   `Drop`.

2. **Rational witnesses: deterministic limits, word-level division.** Each
   witness attempt stopped after 250 ms of wall-clock time, so whether an
   incumbent got certified (and therefore which nodes were pruned) depended
   on machine speed and load. Division was bit-serial, so gcd normalisation
   dominated: on egout, 4 witness calls took 151 ms of a 768 ms solve.
   Division is now Knuth's algorithm D on 32-bit limbs, and the guard is a
   thread-local count of limb operations: 8,000,000 per attempt and
   64,000,000 per solve (new diagnostic `rational_witness_work`). Witness
   outcomes are unchanged on every public model; egout's witness time fell
   to 8 ms. Long division is tested against a bit-serial oracle on
   structured multi-limb operands.

3. **Lifted covers stopped early.** Sequential lifting `break`s when the
   exact profit DP gives up. An outside item heavier than the row capacity
   makes the DP's capacity negative, which it reports the same way, so the
   first such item ended lifting for all later items. Such items are now
   skipped (coefficient 0; the row alone forces them to 0). Test: row
   `3,3,3,9,4 <= 5` still lifts the weight-4 item.

   This changes search wherever such a row is separated. On 300 random
   binary knapsack models (25–35 items, 3–5 rows, some items heavier than
   a row's capacity) with lifted covers on, the cut changed in 285: 148 need
   fewer nodes, 18 more, 1,746 to 1,336 nodes in total, same optima. On 60
   larger ones (60–80 items, 4–8 rows): 31 fewer, 8 more, 520 to 424 nodes.
   Of the public models only p0548 is affected. Its cut on row 103 gains
   five lifted terms (checked against all 91 feasible points of the row);
   the original model's LP value is 315.25 with either cut. Its capped
   100,000-node runs nevertheless end worse: lifted covers reach bound 1,759
   instead of 5,785, and the combined policy reaches 6,195 instead of 6,946
   without the incumbent (20,592) that the delivered run found at node
   93,634.

4. **Feasibility Jump left integers fractional.** Seeds, candidate moves and
   restarts were clamped to the raw column bounds, so an integer column with
   bounds `[0, 2.5]` could move to 2.5. Integer columns now use
   `[ceil(lower), floor(upper)]`.

5. **Root separation counters looked like options.** Seven counters were
   reported without the `diagnostics.` prefix used by every other counter,
   and the initial bound and objective were Rust `Debug` strings
   (`Some(5.0)`). They are now `diagnostics.directed_root_cuts`,
   `.lifted_root_cuts`, `.root_cut_work`, `.root_separation_nodes`,
   `.root_separation_pivots`, `.root_separation_initial_bound` and
   `.root_separation_initial_objective` (a number or `none`). The audit and
   report scripts accept both spellings.

6. **Mixed propagation per-node cost.** Callgrind on bell5 with
   `mixed_propagation` put 68% of instructions in `mixed.rs`, nearly all in
   non-inlined one-term `incumbent::dot` calls, and 18% in libm `trunc`
   (`f64::fract` is a library call on baseline x86-64). The integer test is
   now an exact `i64` round trip, the outward-rounded helpers inline, and the
   propagator reuses its buffers across rows and nodes. Search is
   bit-identical. Same machine, mixed policy: bell5 19.2 s to 7.0 s, egout
   874 to 331 ms, flugpl 62 to 25 ms, rgn 2.44 to 1.32 s.

7. **Legacy root strong branching changed behaviour.** 0.24 let
   `strong_branch` probe a single candidate (useful for reliability
   branching), which also made the opt-in legacy rule
   (`strong_branching=true`) spend two probe LPs on a forced choice. The
   legacy rule probes only when there are two candidates again; a new test
   pins both behaviours.

## Verification

Rerunning `benchmarks/validate_mip_search_024.py` on this branch against
the frozen 6,330-case inputs gives the same status, objective, node count
and pivot count as the frozen 0.24.0 records in all 83,670 runs (15
policies). The frozen-adjudication audit passes: 0 failures, the same 19
raw flags per full policy, and 0 unsafe bounds in the 400-model audit. All
eight public MIPs reproduce the delivered node and pivot counts under the
default, reliability, Feasibility Jump, directed-cover, lifted-cover, mixed
and combined policies, except p0548 under lifted covers and combined, whose
root cut changes (change 3). p0548's directed- and lifted-cover runs hit the
30 s limit on this machine and were rerun with 300 s.

Search changes are therefore limited to lifted covers on rows with items
heavier than the capacity (change 3) and the legacy strong-branching rule
(change 7); both are off by default.

Summed MIP solve seconds over the 2,270 oracle MIPs, same machine:

| Build | Certification on (default) | Certification off |
| --- | ---: | ---: |
| delivered 0.24.0 | 4.7 s | 0.83 s |
| this branch | 1.0 s | 0.68 s |

## Decisions kept

**Reliability branching stays opt-in.** In the frozen data it helps the
larger public models (gt2 72,182 to 16,056 nodes; lseu, rgn and p0548's
capped bound also improve) but costs on small ones. Probes are charged as
nodes, and on the 2,270 oracle MIPs it raises nodes plus pivots in 1,621
cases, lowers them in 20 and leaves 629 equal (geometric mean 1.57x); it is
also worse on flugpl and on bell5's capped bound. A size or delayed-start
gate might keep the gains, but it would be tuned on eight public models.

Feasibility Jump, directed and lifted covers and mixed propagation also
remain opt-in, as delivered.
