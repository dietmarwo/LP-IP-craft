# MIP gap measurements

Each variant uses the same model and a fresh process, with a 10 s solver limit and a 30 s process watchdog. `full` leaves gap stopping off. Time/node limits are retained; they are not proofs of an optimum. Timings are observations from a shared QA host.

| Case | Full status / nodes / ms | 1% status / nodes / ms | 0.01% status / nodes / ms |
| --- | --- | --- | --- |
| bell5 | node_limit / 100000 / 5324.38 | gap_limit / 5452 / 215.57 | node_limit / 100000 / 4844.05 |
| egout | optimal / 3245 / 170.53 | gap_limit / 233 / 13.05 | gap_limit / 274 / 14.33 |
| flugpl | optimal / 25081 / 597.04 | optimal / 25081 / 413.37 | optimal / 25081 / 325.88 |
| gt2 | optimal / 38867 / 2788.92 | optimal / 38867 / 2602.58 | optimal / 38867 / 2320.75 |
| lseu | optimal / 8681 / 321.51 | gap_limit / 7618 / 234.30 | optimal / 8681 / 296.38 |
| p0548 | time_limit / 75445 / 10105.93 | time_limit / 74559 / 10123.92 | time_limit / 74693 / 10124.42 |
| rgn | optimal / 7249 / 407.00 | gap_limit / 6413 / 432.98 | gap_limit / 6847 / 322.12 |
| p01 | optimal / 1 / 1.37 | optimal / 1 / 1.79 | optimal / 1 / 2.27 |
| packing-d3-1024-seed20261017 | optimal / 0 / 45.73 | optimal / 0 / 42.24 | optimal / 0 / 40.22 |
| packing-d9-1024-seed20261017 | optimal / 0 / 221.84 | optimal / 0 / 249.59 | optimal / 0 / 188.88 |
| afiro | optimal / 0 / 0.49 | optimal / 0 / 0.43 | optimal / 0 / 0.40 |
| adlittle | optimal / 0 / 0.69 | optimal / 0 / 0.73 | optimal / 0 / 0.71 |
| israel | optimal / 0 / 7.00 | optimal / 0 / 2.34 | optimal / 0 / 2.37 |
| binary-packing-n32 | optimal / 322 / 9.72 | gap_limit / 302 / 8.89 | optimal / 322 / 6.31 |
| binary-packing-n48 | optimal / 791 / 30.29 | gap_limit / 684 / 26.17 | optimal / 791 / 31.09 |
| binary-packing-n64 | optimal / 7137 / 289.70 | gap_limit / 6621 / 213.29 | optimal / 7137 / 290.44 |

Validation: 48 records, 0 invalid findings. Default numerical records match 0.29.0 on all 16 cases with a fixed 8,192-node budget and no cooperative wall limit (timings and version labels excluded). The 10-second p0548 runs are wall-censored and have different node counts, as expected for time-limited runs.
