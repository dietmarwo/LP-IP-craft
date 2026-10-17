# Native iteration 8 — conservative row redundancy

Version 0.12.0. Before allocating a dictionary, enclose each original row's
activity over the current variable box using outward arithmetic. Remove a
row only if the complete enclosure satisfies both finite row bounds; free
rows can always be omitted. Do not tighten bounds approximately or declare
infeasibility from heuristic arithmetic. Preserve original row numbering and
zero postsolve multipliers for removed rows.

Reuse the elementary arithmetic already used for conservative incumbents,
with a gradual-underflow probe. Disable this presolve if arithmetic is not
supported or an enclosure is unavailable. Add a separate opt-out, and count
removed row instances. Compute the memory preflight after certified removal.

Test equality rows at and beyond the box boundary, decimals, infinities,
nonzero fixed shifts, both senses, explicit zeros, and enabled/disabled
agreement. Run all Rust/Python tests and 600 oracle cases. Measure the 300-row
redundancy fixture and record regressions elsewhere. No HiGHS dependency.

Measurement refinement: the default scan requires m/n >= 1.0; a validated
redundant_row_min_ratio control can force scanning (zero) or raise the threshold.
This avoids the measured scan overhead on row-light models.
