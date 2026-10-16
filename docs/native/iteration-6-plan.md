# Native iteration 6 — borrowed node-bound views

Base native version 0.9.0 at 7a1c811. Version 0.10.0.

Profile the seven fixed fixtures before changes, extending the original four
with a connected binary model, provably redundant rows and separable LP blocks.
Keep identical fixtures through iterations 6–10 and retain a version-5 executable.

Remove full ModelData/matrix copies at branch nodes. Add core Rust residual
and conservative dual-bound functions accepting replacement variable bounds,
with the existing functions forwarding unchanged original bounds. Conversion,
heuristic checking and node-bound certification borrow the original immutable
model plus node-local bounds. There must be no new mutable shared model state.

Test bound-view functions against independently constructed models, malformed
bounds, both objective senses and infinite bounds. Run all Rust/Python suites
and the existing 600-case external/exact oracle. Compare release measurements
with version 5, preserving objectives, nodes and pivot counts. No solver
algorithm or tolerance change is intended, and HiGHS remains absent.
