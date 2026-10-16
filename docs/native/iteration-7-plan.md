# Native iteration 7 — stream CSR into the dictionary

Version 0.11.0. Replace temporary dense Vec<Vec<f64>> matrices, RHS buffers
and per-variable heap allocations with a single checked tableau allocation,
inline variable substitutions, and a reusable sparse row scratch buffer.
Fill only stored nonzero coefficients and compact unused empty rows before
simplex. Apply the same row/objective scaling and preserve postsolve maps.

Keep the dense cap and preflight before allocation, explicit zeros, fixed and
free columns, ranged/equality rows and both senses. No coefficient dropping
or numerical tolerance change. Run all Rust/Python tests and 600 oracle cases;
add a compacted-row/dictionary regression. Compare conversion-heavy and
redundant-row fixtures against iteration 6, preserving node/pivot outcomes.
