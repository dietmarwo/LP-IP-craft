# Bound availability after bulk-import changes

Version 0.4.0, compared with reviewed `main` at `a7a0407`.
Reproduced using `python examples/bound_availability.py --table`.
All finite bounds passed the example's exact rational Lagrangian check.
The availability counts match the reviewed iteration 2 record.

Seed 1, 100 models per family.

| Variable boxes | Multipliers | Finite bounds | Median looseness | Otherwise |
| --- | --- | ---: | ---: | --- |
| box [0, 3] | HiGHS duals | 100 / 100 | 2.5e-15 | - |
| box [0, 3] | margin duals | 100 / 100 | 2.5e-15 | - |
| x >= 0, no upper bound | HiGHS duals | 5 / 100 | 1.4e-16 | unbounded_variable 95 (exact bound of these duals infinite: 79) |
| x >= 0, no upper bound | margin duals | 100 / 100 | 1.4e-06 | - |
| half the columns free | HiGHS duals | 2 / 100 | 1.5e-16 | unbounded_variable 98 (exact bound of these duals infinite: 98) |
| half the columns free | margin duals | 4 / 100 | 1.3e-06 | unbounded_variable 96 (exact bound of these duals infinite: 96) |
| MIP: integers in [0, 4], continuous x >= 0 | HiGHS duals | 15 / 100 | 7.2e-16 | unbounded_variable 85 (exact bound of these duals infinite: 73) |
| MIP: integers in [0, 4], continuous x >= 0 | margin duals | 100 / 100 | 8.2e-02 | - |
