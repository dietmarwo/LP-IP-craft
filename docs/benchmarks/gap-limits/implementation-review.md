# Implementation review: LP-IP item 3

## Commits

The cumulative implementation/evidence patch is based on `e29a4b0a688b4d59724992f969b83e0e6d0c9881`. It includes synchronized 0.30.0 metadata for release preparation; no publication or tag is performed.

## Verification

Native, Python and worker tests cover finite/nonnegative validation; positive absolute/relative limits; zero limits; continuous LP behavior; zero objectives and offsets; separable whole-model scope; rejection of split-tree and specialized packing paths; feasible job claims with gap details; and default compatibility. The full test results are in the combined implementation log. The benchmark retains limited and slower runs as well as early stops.

## Notes for review

The frontier test uses the current node plus the heap and certified closed bounds. A root/node heuristic cannot drop the current relaxation from that bound. A stopped current node is retained in the frontier before exit, preserving the reported bound. The certified incumbent and the displayed candidate must both meet the requested tolerance. A nonpositive or invalid bound distance never creates a `gap_limit` status; closed-frontier optimality follows the existing path.

Positive tolerances bypass component splitting. Root preprocessing must preserve `decompose=false` in the effective report; its metadata overwrite is covered by the Python separable-model regression. Zero controls do not change the search; explicitly supplied controls still appear in effective metadata. The shared job protocol needs no change.

## Merge order

Merge native/status, Python/discovery, tests/evidence and documentation together. Release 0.30.0 only after the maintainer reviews normal package gates. Specialized tree/packing support can be added later; until then those combinations fail explicitly.
