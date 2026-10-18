# Benchmark reference software

The benchmark-only `vendor/ripsolve` tree is from the published ripsolve 0.4.0 source distribution, SHA256 a46e01c8e528e24adf9207f7b17a7ca7442ec03b9fe01e04ab33ddf975641871. Copyright (c) 2026 Andrew Henshaw. MIT licence; the full licence is retained at vendor/ripsolve/LICENSE. Its crate manifest makes inherited metadata explicit so it builds as a path dependency of this standalone harness; the cut separation entry points have optional RS_OFF early exits for family ablations. Unset RS_OFF retains the numerical reference path. All changes from the sdist are in ripsolve-0.4.0-ablation.patch.

The production native changes are independent implementations of standard presolve, GMI, MIR, L1 pump and pseudocost methods. ripsolve source served as an engineering reference. No ripsolve numerical code or dependency is linked into the production Rust crates.

Held-out MPS inputs come from https://github.com/ERGO-Code/HiGHS at commit 73cac48c5340d775a477087198611862559be250. Source hashes and the original files are retained in heldout/manifest.json. The repository's licence is retained as HiGHS-check-LICENSE.txt. Use these test models under the source repository's attribution; they are benchmark data, not solver code.
