# Native iteration 2 — presolve and compact conversion

Version 0.6.0. Eliminate exactly fixed variables before building the dense
standard form; substitute their objective/row contributions and reconstruct
them in postsolve. Drop rows that become identically zero when feasible,
retain contradictory rows for phase-I infeasibility. Keep row multiplier
mapping so independent original-unit checks still apply. Add a memory
preflight before any dense row allocation and preserve explicit zero entries.
Expose a presolve switch for differential tests. No approximate coefficient
or row dropping and no change to acceptance tolerances.

Run existing suites and the 400-case oracle. Test all-fixed feasible and
infeasible models, free rows and fixed variables with nonzero objective and
row contributions. Measure against iteration 1 with the same fixtures; the
fixed-column fixture is the target, while other cases must remain correct.
