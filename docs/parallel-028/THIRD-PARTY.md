# Scheduler dependencies and primary references

Numerical algorithms and independent checks remain owned Rust. Native execution adds exactly Rayon1.12.0 and locked rayon-core/crossbeam/either dependencies. MIT/Apache license texts available in the local registry are retained in `licenses/`; Cargo.lock records exact versions/checksums. The core remains standard-library-only. PyO3 remains optional; no HiGHS/BLAS/C++ solver is linked.

- [Current upstream base](https://github.com/dietmarwo/LP-IP-craft/tree/a1180e06a85685fa82ca977d521ef33f285f8f51).
- [Rayon1.12 local ThreadPoolBuilder](https://docs.rs/rayon/1.12.0/rayon/struct.ThreadPoolBuilder.html) and [owned ThreadPool execution](https://docs.rs/rayon/1.12.0/rayon/struct.ThreadPool.html).
- [Scoped standard-library threads](https://doc.rust-lang.org/std/thread/fn.scope.html), used for existing independent work.
- [SciPy linprog](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.linprog.html) and [milp](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.milp.html), external test oracles only.

The SpOC4 matching tables retain the existing [data sources/ESA GPL attribution](../../benchmarks/spoc4-025/DATA-SOURCES.md) and adjacent ESA license. Vendored comparison-only ripsolve and HiGHS notices remain untouched. This release's performance claims come from delivered code/tests/raw measurements, not assumed properties of these references.
