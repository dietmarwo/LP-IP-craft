# Dependencies

`lp-ip-craft` depends only on Rust's standard library. The owned native LP/MIP algorithms and independent checks remain Rust. Native execution uses exactly Rayon 1.12.0 (MIT OR Apache-2.0), with locked rayon-core, crossbeam-deque/epoch/utils and either. These supply local thread scheduling; no external optimization or linear algebra engine is used. Both project numerical crates forbid unsafe code. Rayon and its dependencies have their own internal implementation and licenses.

The optional Python extension uses PyO3 0.30.1 (MIT OR Apache-2.0) and its Rust macro/build dependencies. Cargo.lock is authoritative. SciPy/HiGHS are external test oracles only, never native runtime dependencies. The checked-in comparison/vendor material and ESA SpOC4 data retain their original license/attribution files; they are not linked into the solver.

Existing independent batches/components/residuals/frontiers use scoped standard-library threads. Numerical pools use pinned pure-Rust Rayon, with no global pool mutation. See `docs/parallel-028/THIRD-PARTY.md` for scheduler references/licenses and `REPRODUCE.md` for actual executed toolchains.

## Shared agent job API

The job API crates (`optimization-job-api`, `optimization-runner`,
`lp-ip-job-worker`) are workspace members but not default members, and the
numerical crates do not depend on them. They add, all pure Rust and resolved
through Cargo.lock: serde and serde_json (with `float_roundtrip`), sha2, libc,
fs2, clap, and tempfile, plus their transitive crates. `cargo tree -p
lp-ip-craft-native` is unchanged and still has no HiGHS, BLAS or LAPACK
dependency. The job layer needs Rust 1.88 and Linux; the numerical crates keep
Rust 1.85. See the [job API guide](job-api.md).
