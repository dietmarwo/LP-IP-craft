Seed 1, 5 models per family.

| Variable boxes | Multipliers | Finite bounds | Median looseness | Otherwise |
| --- | --- | ---: | ---: | --- |
| box [0, 3] | native Rust duals | 5 / 5 | 3.9e-15 | - |
| box [0, 3] | margin duals | 5 / 5 | 3.9e-15 | - |
| x >= 0, no upper bound | native Rust duals | 1 / 5 | -1.4e-16 | unbounded_variable 4 (exact bound of these duals infinite: 4) |
| x >= 0, no upper bound | margin duals | 5 / 5 | 1.8e-06 | - |
| half the columns free | native Rust duals | 0 / 5 | - | unbounded_variable 5 (exact bound of these duals infinite: 5) |
| half the columns free | margin duals | 0 / 5 | - | unbounded_variable 5 (exact bound of these duals infinite: 5) |
| MIP: all variables in [0, 4] | native Rust duals | 5 / 5 | 1.8e-02 | - |
| MIP: all variables in [0, 4] | margin duals | 5 / 5 | 1.8e-02 | - |
