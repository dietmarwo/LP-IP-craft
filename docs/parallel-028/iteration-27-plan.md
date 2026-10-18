# Iteration 27: Tune measured outer scheduling

## Change and purpose

Use all preceding evidence to choose bounded claims/order and small-task fallbacks; remove unhelpful scheduling prototypes.

## Tests and measurement gate

Run paired scalar/eight, batch/components/tree/residual and cold/warm cases; no adjacent-ratio multiplication or cherry-picked replacement. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

## Measured adaptation before implementation

Weighted column splitting 23 gave no reliable gain on either declared skewed primary size (about 45.16→47.80ms and 97.04→97.31ms). Remove it and its per-LP classification. Raise generic pricing dispatch to 262,144 *nonbasic* coefficient operations; avoid parallel dispatch for cheap late bases. Keep large disjoint Schur work and tested outer scheduling. A pre-cancelled batch can use its serial path to avoid helpers while preserving ordinary per-job validation/error precedence.

Code review of the real SpOC4 path shows three additional independent phases in its packing primal-dual root: column gradients, independent projected primal coordinates, and independent row-price projections. Add bounded local execution for these phases when numerical threads are explicitly requested. Column incidence lists are in original row order, so each gradient retains its scalar addition order. Coordinate/row operations retain their original expressions, with joins between phases. Keep step sizes, iteration/work budgets, stopping checks, averaging, greedy order and serial conservative bound checks. Add a35-step active parallel comparison on 32,768 binary columns (including the 32-step certification boundary), requiring bitwise hint/price/bound and identical work counters for 1/2/4/8. This is an application-driven extension before application measurement, not a replacement of old fixtures.
