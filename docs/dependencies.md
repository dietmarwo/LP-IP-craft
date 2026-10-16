# Dependencies

`lp-ip-craft` and `lp-ip-craft-native` depend only on each other and Rust's
standard library. There is no third-party optimization or linear algebra crate.
Both numerical crates forbid unsafe code.

The optional `lp-ip-craft-python` extension depends on PyO3 0.30.1
(MIT OR Apache-2.0) and its Rust build/macro dependencies. Python remains a
separate runtime, and the wheel uses the system C runtime through Rust/Python.
Pure Rust describes the numerical implementation and its dependency graph.

HiGHS, highs-sys, highs, bindgen, CMake and their solver sources were removed
from the workspace and lockfile. SciPy is used only by the optional external
oracle script; it is neither imported by the library nor declared as a package
dependency. Tests use pytest and NumPy. `Cargo.lock` is authoritative.

Independent components use scoped Rust standard-library threads. The numerical
crates still have no third-party dependencies.
