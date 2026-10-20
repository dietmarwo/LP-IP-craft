# Iteration 3 results — efficient HiGHS model loading

Version 0.4.0 · 16 October 2026, America/Lima.
Base: reviewed public `main` at `a7a04074747a99048810204efe0421b4093c9fb4`.
[Plan](iteration-3-plan.md) · [API](../api.md) · [Validation](iteration-3-validation.md).

## Review and implementation

The merged iteration 2 conservative-bound kernel and margin-dual work are
preserved. Before changes, the reviewed base passed 71 Rust tests plus one
doctest and 94 compiled Python tests. The target GTOC application is still
unavailable, so this iteration measures adapter overhead on explicit fixtures.

The new path passes the existing immutable CSR arrays directly into HiGHS's
bulk C API. It removes the `RowProblem` builder's per-column growable vectors,
CSC flattening and additional cost/bound/value copies. The Rust side now
allocates checked native-width CSR index buffers and, only for a MIP, an
integrality vector. Original floating-point arrays are borrowed for the call;
HiGHS copies them and converts CSR to its internal format. This is reduced
copying, not a zero-copy solver.

The existing safe `highs::Model` wrapper owns the native handle. Settings are
applied before import; sense, offset, bounds and domains are loaded together.
The narrowly scoped unsafe boundary documents dimensions, checked index
conversion, pointer lifetimes and the pinned HiGHS copy behavior. Empty rows,
row-free models, free/fixed variables and explicit zero coefficients are covered.
An imported nonzero-count check prevents silent coefficient removal.

No public Rust/Python signature or setting changed. Normal `backend="highs"`
solves use the new path automatically. Independent acceptance checks, starts,
limits, cancellation, status mapping and the conservative-bound algorithm are
unchanged. There is still a fresh native handle per solve. Only package
versions changed in Cargo.lock; external dependency pins are unchanged.

## Correctness defect fixed while removing setup overhead

The old adapter set `small_matrix_value=1e-12` after importing the model.
HiGHS had already applied its default `1e-9` threshold by then. Thus a matrix
coefficient accepted by the adapter could disappear before optimization.

A deterministic reproducer is

```text
minimize x + 7
subject to 2^(-32) x >= 2^(-12),  0 <= x <= 2^21.
```

Its exact optimum is `x=2^20`, objective `2^20+7`. The old code returned
`Infeasible`. The new loader applies the threshold before import and returns
the correct optimum for both LP and integer domains. The maximization variant
with the opposite row side also passes. The same regressions run through Rust
and the compiled Python wrapper. Coefficients at or below the documented
`1e-12` minimum remain explicitly unsupported. Explicit zeros may be removed;
unexpected removal of any nonzero causes a backend error before solving.

## Efficiency measurements

Release abi3 wheels for baseline 0.3.0 and new 0.4.0, installed separately.
Linux x86_64, CPython 3.12.14, Rust 1.99.1, HiGHS 1.15.0. Seven measured runs
per version and case, with alternating baseline/new process order. Each child
warms one solve before measuring one. Compilation was complete before this
comparison; no build or other validation job ran alongside it.

| Fixture | Variables | Rows | Stored entries | Purpose |
| --- | ---: | ---: | ---: | --- |
| Box LP | 100,000 | 0 | 0 | Column-import overhead; analytic optimum |
| Sparse import LP | 40,000 | 40,000 | 200,000 | Sparse loading and presolve; analytic optimum |
| Transportation LP | 4,800 | 140 | 9,600 | Balanced transportation requiring simplex work |
| Partition MIP | 12,000 binary | 2,000 | 12,000 | Assignment by disjoint groups; analytic optimum |

Median times below are milliseconds. End-to-end means the Python `solve` call,
including backend validation, native work and result conversion; it excludes
Python process startup and initial construction of the immutable `Model`.

| Fixture | Setup before | Setup after | Setup ratio | Total before | Total after | Total time reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Box LP | 4.023 | 1.644 | 2.45× | 16.335 | 14.980 | 8.3% |
| Sparse import LP | 11.397 | 4.628 | 2.46× | 103.824 | 91.799 | 11.6% |
| Transportation LP | 0.619 | 0.198 | 3.13× | 12.234 | 11.876 | 2.9% |
| Partition MIP | 1.492 | 0.366 | 4.08× | 62.640 | 61.314 | 2.1% |

All 56 measured solves returned independently accepted optima. Objectives,
iteration counts and node counts are identical across versions and repetitions:
0 for the two import LP objectives, 16,980 for transportation (188 iterations),
and 20,261 for the partition MIP (zero search nodes). The other fixtures use
zero simplex iterations. These easy MIP/import fixtures do not measure hard
branch-and-bound search.

The 8–12% total reduction on large import fixtures is useful, while the
2–3% changes on engine-dominated cases are small relative to timing variation.
Their runtime ranges overlap; do not interpret those small differences as a
general search speedup. Even some unchanged phases move in the opposite
direction (for example, checks on the box LP). This iteration's demonstrated
mechanism is faster setup. No peak-memory reduction or 16-core throughput
number is claimed.

[Raw results](iteration-3-efficiency-results.json) include every run, phase,
objective, residual, iteration/node count, model-construction time, wall-time
range, versions and benchmark-script hash. Phase medians are independent and
need not sum to the median total. The benchmark checks acceptance without
asserting a runtime threshold.

Reproduce using the new wheel and a separate baseline environment:

```bash
python examples/benchmark_efficiency.py --repeats 7 \
    --baseline-python /path/to/baseline-venv/bin/python > comparison.json
```

Run without `--baseline-python` for a single-version profile. CI includes an
acceptance smoke check on these fixtures, not a speed gate.

## Correctness and compatibility results

- **74 Rust tests plus one doctest pass.** New regressions cover small
  coefficients, sparse boundary cases and rejection of unexpected coefficient
  removal. Existing analytic/random LP checks and exhaustive integer oracles
  remain intact.
- **102 compiled Python tests pass.** The feature-disabled build passes 71
  tests and skips 31 HiGHS-only cases; unavailable backend dispatch raises
  `NotImplementedError`.
- Formatting, native/dummy Clippy with warnings denied, rustdoc with warnings
  denied, Rust 1.85.0 compatibility and strict Python mypy checks pass.
  Both Rust examples and all four existing Python examples pass.
- [Bound availability](iteration-3-bound-availability.md) reproduces reviewed
  iteration 2: ordinary duals give finite bounds on 100/100 bounded-box,
  5/100 one-sided, 2/100 partly free and 15/100 mixed-integer relaxation cases.
  Margin duals give 100/100, 100/100, 4/100 and 100/100 respectively. Every
  finite result passes the example's exact rational Lagrangian check.

See [validation](iteration-3-validation.md) for commands, environment and limits.
The earlier review's 210,000-case external oracle is historical evidence;
this delivery reruns the checked-in exact-oracle suites and examples, not that
uncommitted review probe. Cross-platform CI is configured but was not executed
on GitHub during this task.

## Remaining efficiency work

HiGHS engine time dominates the transportation and partition cases. Improving
it requires actual workload evidence: repeated related LPs could justify basis
reuse and incremental objective/bound changes; independent models could justify
batching or a 16-worker throughput study. The current change reduces import
work without adding persistent solver state. It does not implement a native
Rust simplex/MIP engine, cut generation, parallel search, or safe pruning.
The GTOC target card remains deliberately incomplete.
