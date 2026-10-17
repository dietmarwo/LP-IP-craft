# Added coverage

`public/` retains original MPS and canonical CSR conversions of gt2, lseu,
p01, p0548, rgn and small_mip from the pinned HiGHS 1.15.0 reference source
(`check/instances`). HiGHS is MIT licensed; its retained license is in
`HiGHS-LICENSE.txt` alongside this file. Per-file source hashes, dimensions,
conversion description and untimed SciPy/HiGHS references are in the manifest.
Infinite bounds remain null in JSON; no finite boxes are guessed.

`fresh-packing/` contains six independent holdouts with degree three or nine,
1024 or 4096 variables, shuffled row/column order and fresh positive planted
dual weights. x=1 and the planted dual vector have matching objectives.
Seeds, degrees, construction method and hashes are retained.

These are reference and synthetic benchmarks, not a captured GTOC application.
Failure to solve an original within the common cap remains visible.
