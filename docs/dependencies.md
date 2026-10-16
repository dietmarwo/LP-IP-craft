# Third-party dependencies and notices

Project code is MIT (see [LICENSE](../LICENSE)). Dependencies keep their own
licenses; `Cargo.lock` records the complete resolved graph.

| Component | Version | License | Role | Notice |
| --- | --- | --- | --- | --- |
| HiGHS (C++ solver) | 1.15.0 | MIT | LP/MIP engine, built from source and linked statically | [text](licenses/HiGHS-1.15.0-LICENSE.txt) |
| `highs-sys` | 1.15.0 | MIT | Bundles the HiGHS sources, builds them (CMake) and generates the C API bindings (bindgen) | covered by the HiGHS text |
| `highs` | 2.4.0 | MIT | Safe Rust bindings used by the adapter | [text](licenses/highs-rs-2.4.0-LICENSE.txt) |
| PyO3 | 0.30.1 | MIT OR Apache-2.0 | Python binding | upstream |

The core crate `lp-ip-craft` has no dependencies. The HiGHS adapter and the
Python extension's default `highs` feature pull in the native build. A wheel
built with the default features contains HiGHS; the license texts above are
packaged with it (`license-files` in `pyproject.toml`). Nothing is published.
