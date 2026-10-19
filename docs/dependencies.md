# Dependencies

`lp-ip-craft` depends only on Rust's standard library. The owned native LP/MIP algorithms and independent checks remain Rust. Native execution uses exactly Rayon 1.12.0 (MIT OR Apache-2.0), with locked rayon-core, crossbeam-deque/epoch/utils and either. These supply local thread scheduling; no external optimization or linear algebra engine is used. Both project numerical crates forbid unsafe code. Rayon and its dependencies have their own internal implementation and licenses.

The optional Python extension uses PyO3 0.30.1 (MIT OR Apache-2.0) and its Rust macro/build dependencies. Cargo.lock is authoritative. SciPy/HiGHS are external test oracles only, never native runtime dependencies. The checked-in comparison/vendor material and ESA SpOC4 data retain their original license/attribution files; they are not linked into the solver.

Existing independent batches/components/residuals/frontiers use scoped standard-library threads. Numerical pools use pinned pure-Rust Rayon, with no global pool mutation. See `docs/parallel-028/THIRD-PARTY.md` for scheduler references/licenses and `REPRODUCE.md` for actual executed toolchains.

## Shared agent job API

For durable native jobs with an explicit CPU budget, use the
[shared protocol and runner](job-api.md) and the
[solver-specific tutorial](../tutorials/job-api/README.md).
The job profile is a deliberately smaller, versioned surface than the full
in-process solver API. Unknown profile options are rejected.
