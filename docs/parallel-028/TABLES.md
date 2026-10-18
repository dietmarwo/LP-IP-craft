# Recomputed final measurements

Medians; all measured samples and warmups pass the same original-model/known-answer checks. API/kernel time, full process CPU and workflow have distinct scopes. Larger one/eight ratios are faster. Small and slower observations are retained.

## final-matrix

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | packing | 512 | single | 1 | 1 | 1 | 4.629 | 6.448 | 3 |
| final | packing | 512 | single | 2 | 1 | 1 | 4.887 | 6.877 | 3 |
| final | packing | 512 | single | 4 | 1 | 1 | 4.572 | 6.731 | 3 |
| final | packing | 512 | single | 8 | 1 | 1 | 4.762 | 6.807 | 3 |
| final | packing | 1024 | single | 1 | 1 | 1 | 23.993 | 25.801 | 3 |
| final | packing | 1024 | single | 2 | 1 | 1 | 25.136 | 27.035 | 3 |
| final | packing | 1024 | single | 4 | 1 | 1 | 26.108 | 28.963 | 3 |
| final | packing | 1024 | single | 8 | 1 | 1 | 26.470 | 29.405 | 3 |
| final | irregular | 128 | single | 1 | 1 | 1 | 1.901 | 4.247 | 3 |
| final | irregular | 128 | single | 2 | 1 | 1 | 1.818 | 3.672 | 3 |
| final | irregular | 128 | single | 4 | 1 | 1 | 1.782 | 3.547 | 3 |
| final | irregular | 128 | single | 8 | 1 | 1 | 1.650 | 3.000 | 3 |
| final | irregular | 256 | single | 1 | 1 | 1 | 6.217 | 7.884 | 3 |
| final | irregular | 256 | single | 2 | 1 | 1 | 6.214 | 8.287 | 3 |
| final | irregular | 256 | single | 4 | 1 | 1 | 6.359 | 8.442 | 3 |
| final | irregular | 256 | single | 8 | 1 | 1 | 6.292 | 8.129 | 3 |
| final | selection | 20 | single | 1 | 1 | 1 | 0.544 | 2.141 | 3 |
| final | selection | 20 | single | 2 | 1 | 1 | 0.453 | 1.918 | 3 |
| final | selection | 20 | single | 4 | 1 | 1 | 0.487 | 2.363 | 3 |
| final | selection | 20 | single | 8 | 1 | 1 | 0.474 | 2.404 | 3 |
| final | selection | 28 | single | 1 | 1 | 1 | 0.412 | 2.060 | 3 |
| final | selection | 28 | single | 2 | 1 | 1 | 0.431 | 2.358 | 3 |
| final | selection | 28 | single | 4 | 1 | 1 | 0.370 | 1.802 | 3 |
| final | selection | 28 | single | 8 | 1 | 1 | 0.425 | 2.243 | 3 |
| final | dense | 192 | single | 1 | 1 | 1 | 15.395 | 18.229 | 3 |
| final | dense | 192 | single | 2 | 1 | 1 | 16.183 | 19.197 | 3 |
| final | dense | 192 | single | 4 | 1 | 1 | 16.566 | 19.829 | 3 |
| final | dense | 192 | single | 8 | 1 | 1 | 15.887 | 18.539 | 3 |
| final | dense | 384 | single | 1 | 1 | 1 | 119.148 | 124.504 | 3 |
| final | dense | 384 | single | 2 | 1 | 1 | 114.780 | 129.845 | 3 |
| final | dense | 384 | single | 4 | 1 | 1 | 119.911 | 144.121 | 3 |
| final | dense | 384 | single | 8 | 1 | 1 | 116.453 | 148.491 | 3 |
| final | packing | 128 | batch | 1 | 1 | 64 | 36.889 | 40.929 | 3 |
| final | packing | 128 | batch | 1 | 2 | 64 | 20.248 | 41.443 | 3 |
| final | packing | 128 | batch | 1 | 4 | 64 | 11.980 | 48.302 | 3 |
| final | packing | 128 | batch | 1 | 8 | 64 | 7.549 | 53.616 | 3 |
| final | packing | 128 | skewbatch | 1 | 1 | 64 | 96.594 | 101.780 | 3 |
| final | packing | 128 | skewbatch | 1 | 2 | 64 | 53.268 | 108.415 | 3 |
| final | packing | 128 | skewbatch | 1 | 4 | 64 | 26.824 | 103.507 | 3 |
| final | packing | 128 | skewbatch | 1 | 8 | 64 | 17.024 | 120.178 | 3 |
| final | packing | 128 | components | 1 | 1 | 1 | 10.065 | 12.373 | 3 |
| final | packing | 128 | components | 1 | 2 | 1 | 5.651 | 6.558 | 3 |
| final | packing | 128 | components | 1 | 4 | 1 | 4.423 | 8.740 | 3 |
| final | packing | 128 | components | 1 | 8 | 1 | 5.773 | 12.896 | 3 |
| final | packing | 128 | skewcomponents | 1 | 1 | 1 | 18.438 | 21.057 | 3 |
| final | packing | 128 | skewcomponents | 1 | 2 | 1 | 11.521 | 17.106 | 3 |
| final | packing | 128 | skewcomponents | 1 | 4 | 1 | 10.203 | 26.145 | 3 |
| final | packing | 128 | skewcomponents | 1 | 8 | 1 | 10.279 | 27.257 | 3 |
| final | selection | 28 | tree | 1 | 1 | 1 | 0.423 | 2.137 | 3 |
| final | selection | 28 | tree | 1 | 2 | 1 | 1.137 | 2.278 | 3 |
| final | selection | 28 | tree | 1 | 4 | 1 | 0.844 | 2.916 | 3 |
| final | selection | 28 | tree | 1 | 8 | 1 | 0.997 | 4.507 | 3 |
| final | ring | 64 | residual | 1 | 1 | 1 | 2.605 | 4.139 | 3 |
| final | ring | 64 | residual | 1 | 2 | 1 | 2.434 | 4.066 | 3 |
| final | ring | 64 | residual | 1 | 4 | 1 | 2.222 | 5.397 | 3 |
| final | ring | 64 | residual | 1 | 8 | 1 | 2.425 | 7.248 | 3 |

## common-upstream

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | packing | 512 | single | 1 | 1 | 1 | 5.198 | 6.746 | 3 |
| baseline | packing | 512 | single | 1 | 1 | 1 | 5.382 | 7.028 | 3 |
| final | packing | 1024 | single | 1 | 1 | 1 | 25.310 | 27.280 | 3 |
| baseline | packing | 1024 | single | 1 | 1 | 1 | 25.974 | 27.945 | 3 |
| final | irregular | 128 | single | 1 | 1 | 1 | 1.777 | 3.345 | 3 |
| baseline | irregular | 128 | single | 1 | 1 | 1 | 1.702 | 3.371 | 3 |
| final | irregular | 256 | single | 1 | 1 | 1 | 6.248 | 8.626 | 3 |
| baseline | irregular | 256 | single | 1 | 1 | 1 | 6.918 | 8.923 | 3 |
| final | selection | 20 | single | 1 | 1 | 1 | 0.563 | 2.563 | 3 |
| baseline | selection | 20 | single | 1 | 1 | 1 | 0.633 | 3.123 | 3 |
| final | selection | 28 | single | 1 | 1 | 1 | 0.431 | 2.104 | 3 |
| baseline | selection | 28 | single | 1 | 1 | 1 | 0.419 | 1.826 | 3 |
| final | dense | 192 | single | 1 | 1 | 1 | 15.931 | 18.526 | 3 |
| baseline | dense | 192 | single | 1 | 1 | 1 | 16.428 | 18.873 | 3 |
| final | dense | 384 | single | 1 | 1 | 1 | 129.094 | 134.594 | 3 |
| baseline | dense | 384 | single | 1 | 1 | 1 | 113.610 | 118.367 | 3 |
| final | packing | 128 | batch | 1 | 1 | 64 | 40.669 | 46.215 | 3 |
| baseline | packing | 128 | batch | 1 | 1 | 64 | 36.208 | 40.278 | 3 |
| final | packing | 128 | batch | 1 | 8 | 64 | 7.787 | 55.568 | 3 |
| baseline | packing | 128 | batch | 1 | 8 | 64 | 8.891 | 49.828 | 3 |
| final | packing | 128 | skewbatch | 1 | 1 | 64 | 94.582 | 99.701 | 3 |
| baseline | packing | 128 | skewbatch | 1 | 1 | 64 | 95.148 | 100.717 | 3 |
| final | packing | 128 | skewbatch | 1 | 8 | 64 | 21.670 | 131.227 | 3 |
| baseline | packing | 128 | skewbatch | 1 | 8 | 64 | 21.408 | 135.548 | 3 |
| final | packing | 128 | components | 1 | 1 | 1 | 10.460 | 12.906 | 3 |
| baseline | packing | 128 | components | 1 | 1 | 1 | 9.681 | 13.434 | 3 |
| final | packing | 128 | components | 1 | 8 | 1 | 4.110 | 17.097 | 3 |
| baseline | packing | 128 | components | 1 | 8 | 1 | 3.849 | 11.896 | 3 |
| final | packing | 128 | skewcomponents | 1 | 1 | 1 | 17.998 | 20.748 | 3 |
| baseline | packing | 128 | skewcomponents | 1 | 1 | 1 | 17.570 | 19.975 | 3 |
| final | packing | 128 | skewcomponents | 1 | 8 | 1 | 8.762 | 24.789 | 3 |
| baseline | packing | 128 | skewcomponents | 1 | 8 | 1 | 10.358 | 27.169 | 3 |
| final | selection | 28 | tree | 1 | 1 | 1 | 0.410 | 2.126 | 3 |
| baseline | selection | 28 | tree | 1 | 1 | 1 | 0.513 | 2.134 | 3 |
| final | selection | 28 | tree | 1 | 8 | 1 | 1.800 | 5.042 | 3 |
| baseline | selection | 28 | tree | 1 | 8 | 1 | 1.087 | 5.453 | 3 |

## heldout

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | packing | 768 | single | 1 | 1 | 1 | 10.484 | 12.238 | 3 |
| final | packing | 768 | single | 8 | 1 | 1 | 9.747 | 11.395 | 3 |
| final | packing | 1536 | single | 1 | 1 | 1 | 33.650 | 35.802 | 3 |
| final | packing | 1536 | single | 8 | 1 | 1 | 31.753 | 33.919 | 3 |
| final | irregular | 160 | single | 1 | 1 | 1 | 2.189 | 3.703 | 3 |
| final | irregular | 160 | single | 8 | 1 | 1 | 2.361 | 4.068 | 3 |
| final | irregular | 320 | single | 1 | 1 | 1 | 10.523 | 12.178 | 3 |
| final | irregular | 320 | single | 8 | 1 | 1 | 10.117 | 12.016 | 3 |
| final | selection | 24 | single | 1 | 1 | 1 | 0.361 | 1.708 | 3 |
| final | selection | 24 | single | 8 | 1 | 1 | 0.341 | 1.660 | 3 |
| final | selection | 30 | single | 1 | 1 | 1 | 0.518 | 1.939 | 3 |
| final | selection | 30 | single | 8 | 1 | 1 | 0.488 | 1.963 | 3 |
| final | dense | 256 | single | 1 | 1 | 1 | 38.588 | 42.092 | 3 |
| final | dense | 256 | single | 8 | 1 | 1 | 46.188 | 56.651 | 3 |
| final | dense | 512 | single | 1 | 1 | 1 | 296.019 | 302.871 | 3 |
| final | dense | 512 | single | 8 | 1 | 1 | 299.163 | 408.315 | 3 |
| final | packing | 96 | components | 1 | 1 | 1 | 6.565 | 8.408 | 3 |
| final | packing | 96 | components | 1 | 8 | 1 | 2.642 | 10.871 | 3 |
| final | packing | 96 | skewcomponents | 1 | 1 | 1 | 12.089 | 14.399 | 3 |
| final | packing | 96 | skewcomponents | 1 | 8 | 1 | 7.313 | 13.670 | 3 |
| final | packing | 160 | components | 1 | 1 | 1 | 13.338 | 15.722 | 3 |
| final | packing | 160 | components | 1 | 8 | 1 | 5.095 | 15.254 | 3 |
| final | packing | 160 | skewcomponents | 1 | 1 | 1 | 24.481 | 26.946 | 3 |
| final | packing | 160 | skewcomponents | 1 | 8 | 1 | 13.446 | 31.805 | 3 |

## factor-scaling

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | factor | 384 | single | 1 | 1 | 3 | 13.276 | 22.794 | 7 |
| final | factor | 384 | single | 2 | 1 | 3 | 11.713 | 25.512 | 7 |
| final | factor | 384 | single | 4 | 1 | 3 | 10.610 | 29.349 | 7 |
| final | factor | 384 | single | 8 | 1 | 3 | 13.884 | 41.567 | 7 |
| final | factor | 512 | single | 1 | 1 | 3 | 37.557 | 53.768 | 7 |
| final | factor | 512 | single | 2 | 1 | 3 | 30.004 | 55.322 | 7 |
| final | factor | 512 | single | 4 | 1 | 3 | 26.660 | 61.327 | 7 |
| final | factor | 512 | single | 8 | 1 | 3 | 32.661 | 85.004 | 7 |
| final | factor | 768 | single | 1 | 1 | 3 | 108.449 | 147.675 | 7 |
| final | factor | 768 | single | 2 | 1 | 3 | 72.729 | 153.848 | 7 |
| final | factor | 768 | single | 4 | 1 | 3 | 59.262 | 182.236 | 7 |
| final | factor | 768 | single | 8 | 1 | 3 | 54.805 | 212.707 | 7 |
| final | factor | 1024 | single | 1 | 1 | 3 | 271.485 | 330.711 | 7 |
| final | factor | 1024 | single | 2 | 1 | 3 | 173.318 | 333.959 | 7 |
| final | factor | 1024 | single | 4 | 1 | 3 | 154.221 | 445.024 | 7 |
| final | factor | 1024 | single | 8 | 1 | 3 | 141.568 | 520.407 | 7 |

## large-solver-scaling

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | dense | 768 | single | 1 | 1 | 1 | 1180.247 | 1194.383 | 3 |
| final | dense | 768 | single | 2 | 1 | 1 | 1029.683 | 1201.393 | 3 |
| final | dense | 768 | single | 4 | 1 | 1 | 1013.340 | 1361.420 | 3 |
| final | dense | 768 | single | 8 | 1 | 1 | 1006.799 | 1456.919 | 3 |
| final | dense | 1024 | single | 1 | 1 | 1 | 3344.793 | 3368.259 | 3 |
| final | dense | 1024 | single | 2 | 1 | 1 | 2694.911 | 3227.967 | 3 |
| final | dense | 1024 | single | 4 | 1 | 1 | 2472.369 | 3393.189 | 3 |
| final | dense | 1024 | single | 8 | 1 | 1 | 2567.954 | 4052.719 | 3 |

## mixed-batches

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | dense | 384 | batch | 1 | 1 | 16 | 1863.187 | 1907.147 | 3 |
| final | dense | 384 | batch | 1 | 2 | 16 | 1050.875 | 2112.652 | 3 |
| final | dense | 384 | batch | 1 | 4 | 16 | 633.338 | 2383.481 | 3 |
| final | dense | 384 | batch | 1 | 8 | 16 | 379.253 | 2769.731 | 3 |
| final | dense | 384 | batch | 2 | 4 | 16 | 656.630 | 2603.392 | 3 |
| final | dense | 384 | batch | 4 | 2 | 16 | 1094.184 | 2442.915 | 3 |
| final | dense | 384 | batch | 8 | 1 | 16 | 1974.765 | 2483.936 | 3 |

## scheduling-confirmation

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| final | packing | 128 | skewbatch | 1 | 8 | 36 | 10.974 | 78.227 | 7 |
| baseline | packing | 128 | skewbatch | 1 | 8 | 36 | 15.443 | 67.815 | 7 |
| final | packing | 128 | batch | 1 | 8 | 128 | 16.792 | 110.069 | 7 |
| baseline | packing | 128 | batch | 1 | 8 | 128 | 15.667 | 100.614 | 7 |
| final | packing | 128 | skewcomponents | 1 | 8 | 1 | 9.699 | 22.865 | 7 |
| baseline | packing | 128 | skewcomponents | 1 | 8 | 1 | 9.630 | 23.894 | 7 |

## scalar-isolation-production

| Build | Family | Size | Mode | Inner | Outer | Jobs | API/kernel ms | Process CPU ms | Repeats |
|---|---|---|---|---|---|---|---|---|---|
| baseline | packing | 128 | single | 1 | 1 | 1 | 0.651 | 2.035 | 7 |
| before | packing | 128 | single | 1 | 1 | 1 | 0.765 | 2.475 | 7 |
| final | packing | 128 | single | 1 | 1 | 1 | 0.698 | 2.132 | 7 |
| baseline | packing | 512 | single | 1 | 1 | 1 | 4.727 | 6.652 | 7 |
| before | packing | 512 | single | 1 | 1 | 1 | 4.976 | 6.643 | 7 |
| final | packing | 512 | single | 1 | 1 | 1 | 5.287 | 6.908 | 7 |
| baseline | packing | 128 | batch | 1 | 8 | 128 | 16.715 | 109.478 | 7 |
| before | packing | 128 | batch | 1 | 8 | 128 | 15.912 | 107.661 | 7 |
| final | packing | 128 | batch | 1 | 8 | 128 | 15.240 | 103.207 | 7 |
| baseline | packing | 128 | skewcomponents | 1 | 8 | 1 | 10.736 | 26.622 | 7 |
| before | packing | 128 | skewcomponents | 1 | 8 | 1 | 9.237 | 23.458 | 7 |
| final | packing | 128 | skewcomponents | 1 | 8 | 1 | 9.130 | 24.135 | 7 |
| baseline | dense | 384 | single | 1 | 1 | 1 | 119.730 | 124.857 | 7 |
| before | dense | 384 | single | 1 | 1 | 1 | 116.545 | 121.110 | 7 |
| final | dense | 384 | single | 1 | 1 | 1 | 119.175 | 124.768 | 7 |

## applications-public

| Case | Build | Inner | Outer | API ms | Workflow ms | Root phase ms | Statuses | Objectives |
|---|---|---|---|---|---|---|---|---|
| avgas | baseline | 1 | 1 | 0.195 | 3.022 | 0.000 | optimal,optimal,optimal | -7.75,-7.75,-7.75 |
| avgas | final | 1 | 1 | 0.236 | 3.357 | 0.000 | optimal,optimal,optimal | -7.75,-7.75,-7.75 |
| avgas | final | 8 | 1 | 0.223 | 3.257 | 0.000 | optimal,optimal,optimal | -7.75,-7.75,-7.75 |
| bell5 | baseline | 1 | 1 | 1014.043 | 1016.720 | 0.000 | time_limit,time_limit,time_limit | 8966933.66832,8966933.66832,8966933.66832 |
| bell5 | final | 1 | 1 | 1017.855 | 1020.469 | 0.000 | time_limit,time_limit,time_limit | 8966933.66832,8966933.66832,8966933.66832 |
| bell5 | final | 8 | 1 | 1018.902 | 1021.456 | 0.000 | time_limit,time_limit,time_limit | 8966933.66832,8966933.66832,8966933.66832 |
| egout | baseline | 1 | 1 | 9.053 | 11.881 | 0.000 | optimal,optimal,optimal | 568.1007000000001,568.1007000000001,568.1007000000001 |
| egout | final | 1 | 1 | 9.272 | 12.870 | 0.000 | optimal,optimal,optimal | 568.1007000000001,568.1007000000001,568.1007000000001 |
| egout | final | 8 | 1 | 9.290 | 12.694 | 0.000 | optimal,optimal,optimal | 568.1007000000001,568.1007000000001,568.1007000000001 |
| flugpl | baseline | 1 | 1 | 247.953 | 251.356 | 0.000 | optimal,optimal,optimal | 1201500.0,1201500.0,1201500.0 |
| flugpl | final | 1 | 1 | 243.594 | 247.016 | 0.000 | optimal,optimal,optimal | 1201500.0,1201500.0,1201500.0 |
| flugpl | final | 8 | 1 | 250.072 | 253.588 | 0.000 | optimal,optimal,optimal | 1201500.0,1201500.0,1201500.0 |
| gt2 | baseline | 1 | 1 | 1016.867 | 1019.345 | 0.000 | time_limit,time_limit,time_limit | 27105.0,27105.0,27105.0 |
| gt2 | final | 1 | 1 | 1014.878 | 1017.636 | 0.000 | time_limit,time_limit,time_limit | 27105.0,27105.0,27105.0 |
| gt2 | final | 8 | 1 | 1017.870 | 1020.481 | 0.000 | time_limit,time_limit,time_limit | 27105.0,27105.0,27105.0 |
| lseu | baseline | 1 | 1 | 253.397 | 256.365 | 0.000 | optimal,optimal,optimal | 1120.0,1120.0,1120.0 |
| lseu | final | 1 | 1 | 251.905 | 255.572 | 0.000 | optimal,optimal,optimal | 1120.0,1120.0,1120.0 |
| lseu | final | 8 | 1 | 258.411 | 262.074 | 0.000 | optimal,optimal,optimal | 1120.0,1120.0,1120.0 |
| p0548 | baseline | 1 | 1 | 1007.565 | 1010.797 | 0.000 | time_limit,time_limit,time_limit | 26667.0,26667.0,26667.0 |
| p0548 | final | 1 | 1 | 1008.174 | 1011.328 | 0.000 | time_limit,time_limit,time_limit | 26667.0,26667.0,26667.0 |
| p0548 | final | 8 | 1 | 1007.769 | 1011.618 | 0.000 | time_limit,time_limit,time_limit | 26667.0,26667.0,26667.0 |
| p01 | baseline | 1 | 1 | 1.339 | 4.154 | 0.000 | optimal,optimal,optimal | 263.0,263.0,263.0 |
| p01 | final | 1 | 1 | 1.236 | 4.393 | 0.000 | optimal,optimal,optimal | 263.0,263.0,263.0 |
| p01 | final | 8 | 1 | 1.227 | 4.453 | 0.000 | optimal,optimal,optimal | 263.0,263.0,263.0 |
| rgn | baseline | 1 | 1 | 348.728 | 352.749 | 0.000 | optimal,optimal,optimal | 82.19999923999995,82.19999923999995,82.19999923999995 |
| rgn | final | 1 | 1 | 388.679 | 392.681 | 0.000 | optimal,optimal,optimal | 82.19999923999995,82.19999923999995,82.19999923999995 |
| rgn | final | 8 | 1 | 337.810 | 341.838 | 0.000 | optimal,optimal,optimal | 82.19999923999995,82.19999923999995,82.19999923999995 |
| 2171 | baseline | 1 | 1 | 4.721 | 8.089 | 0.000 | optimal,optimal,optimal | -22375.758546075085,-22375.758546075085,-22375.758546075085 |
| 2171 | final | 1 | 1 | 4.625 | 7.969 | 0.000 | optimal,optimal,optimal | -22375.758546075085,-22375.758546075085,-22375.758546075085 |
| 2171 | final | 8 | 1 | 4.898 | 8.515 | 0.000 | optimal,optimal,optimal | -22375.758546075085,-22375.758546075085,-22375.758546075085 |
| bound_implied | baseline | 1 | 1 | 0.509 | 3.252 | 0.000 | optimal,optimal,optimal | 0.0,0.0,0.0 |
| bound_implied | final | 1 | 1 | 0.539 | 4.051 | 0.000 | optimal,optimal,optimal | 0.0,0.0,0.0 |
| bound_implied | final | 8 | 1 | 0.514 | 3.512 | 0.000 | optimal,optimal,optimal | 0.0,0.0,0.0 |
| infeasible-mip0 | baseline | 1 | 1 | 0.589 | 3.069 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| infeasible-mip0 | final | 1 | 1 | 0.622 | 3.498 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| infeasible-mip0 | final | 8 | 1 | 0.610 | 3.333 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| infeasible-mip1 | baseline | 1 | 1 | 3.233 | 5.701 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| infeasible-mip1 | final | 1 | 1 | 3.487 | 6.671 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| infeasible-mip1 | final | 8 | 1 | 3.606 | 6.384 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2095 | baseline | 1 | 1 | 1.381 | 4.400 | 0.000 | optimal,optimal,optimal | 3.0,3.0,3.0 |
| issue-2095 | final | 1 | 1 | 1.290 | 4.570 | 0.000 | optimal,optimal,optimal | 3.0,3.0,3.0 |
| issue-2095 | final | 8 | 1 | 1.301 | 4.533 | 0.000 | optimal,optimal,optimal | 3.0,3.0,3.0 |
| issue-2173 | baseline | 1 | 1 | 122.421 | 126.163 | 0.000 | optimal,optimal,optimal | -26770.80754885255,-26770.80754885255,-26770.80754885255 |
| issue-2173 | final | 1 | 1 | 120.416 | 124.501 | 0.000 | optimal,optimal,optimal | -26770.80754885255,-26770.80754885255,-26770.80754885255 |
| issue-2173 | final | 8 | 1 | 127.611 | 131.846 | 0.000 | optimal,optimal,optimal | -26770.80754885255,-26770.80754885255,-26770.80754885255 |
| issue-2204 | baseline | 1 | 1 | 0.314 | 3.134 | 0.000 | optimal,optimal,optimal | 6.0,6.0,6.0 |
| issue-2204 | final | 1 | 1 | 0.371 | 3.288 | 0.000 | optimal,optimal,optimal | 6.0,6.0,6.0 |
| issue-2204 | final | 8 | 1 | 0.427 | 3.794 | 0.000 | optimal,optimal,optimal | 6.0,6.0,6.0 |
| issue-2290 | baseline | 1 | 1 | 0.170 | 2.791 | 0.000 | error,error,error | None,None,None |
| issue-2290 | final | 1 | 1 | 0.194 | 3.005 | 0.000 | error,error,error | None,None,None |
| issue-2290 | final | 8 | 1 | 0.189 | 2.919 | 0.000 | error,error,error | None,None,None |
| issue-2402 | baseline | 1 | 1 | 0.766 | 3.967 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2402 | final | 1 | 1 | 0.779 | 3.574 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2402 | final | 8 | 1 | 0.762 | 3.648 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2874-3 | baseline | 1 | 1 | 0.255 | 2.813 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2874-3 | final | 1 | 1 | 0.275 | 3.009 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| issue-2874-3 | final | 8 | 1 | 0.316 | 4.332 | 0.000 | infeasible,infeasible,infeasible | None,None,None |
| sp150x300d | baseline | 1 | 1 | 1004.781 | 1008.266 | 0.000 | time_limit,time_limit,time_limit | 71.0,71.0,71.0 |
| sp150x300d | final | 1 | 1 | 1005.282 | 1009.622 | 0.000 | time_limit,time_limit,time_limit | 71.0,71.0,71.0 |
| sp150x300d | final | 8 | 1 | 1004.986 | 1008.662 | 0.000 | time_limit,time_limit,time_limit | 71.0,71.0,71.0 |

## applications-spoc

| Case | Build | Inner | Outer | API ms | Workflow ms | Root phase ms | Statuses | Objectives |
|---|---|---|---|---|---|---|---|---|
| i/root | baseline | 1 | 1 | 50.300 | 303.052 | 21.803 | time_limit,time_limit,time_limit | 31735021.0,31735021.0,31735021.0 |
| i/root | final | 1 | 1 | 49.849 | 275.679 | 21.809 | time_limit,time_limit,time_limit | 31735021.0,31735021.0,31735021.0 |
| i/root | final | 8 | 1 | 48.447 | 275.095 | 20.834 | time_limit,time_limit,time_limit | 31735021.0,31735021.0,31735021.0 |
| i/repair | baseline | 1 | 1 | 73.033 | 326.719 | 21.705 | time_limit,time_limit,time_limit | 31743455.0,31743455.0,31743455.0 |
| i/repair | final | 1 | 1 | 67.443 | 313.030 | 20.841 | time_limit,time_limit,time_limit | 31743455.0,31743455.0,31743455.0 |
| i/repair | final | 8 | 1 | 68.191 | 320.663 | 20.887 | time_limit,time_limit,time_limit | 31743455.0,31743455.0,31743455.0 |
| i/repair | baseline | 1 | 8 | 57.775 | 284.553 | 22.425 | time_limit,time_limit,time_limit | 31743455.0,31743455.0,31743455.0 |
| i/repair | final | 1 | 8 | 58.986 | 295.665 | 22.825 | time_limit,time_limit,time_limit | 31743455.0,31743455.0,31743455.0 |
| ii/root | baseline | 1 | 1 | 233.341 | 761.363 | 105.165 | time_limit,time_limit,time_limit | 66952926.0,66952926.0,66952926.0 |
| ii/root | final | 1 | 1 | 241.300 | 801.194 | 102.864 | time_limit,time_limit,time_limit | 66952926.0,66952926.0,66952926.0 |
| ii/root | final | 8 | 1 | 203.000 | 746.033 | 76.106 | time_limit,time_limit,time_limit | 66952926.0,66952926.0,66952926.0 |
| ii/repair | baseline | 1 | 1 | 317.103 | 852.524 | 104.646 | time_limit,time_limit,time_limit | 66961612.0,66961612.0,66961612.0 |
| ii/repair | final | 1 | 1 | 334.346 | 871.372 | 106.376 | time_limit,time_limit,time_limit | 66961612.0,66961612.0,66961612.0 |
| ii/repair | final | 8 | 1 | 303.896 | 844.080 | 74.380 | time_limit,time_limit,time_limit | 66961612.0,66961612.0,66961612.0 |
| ii/repair | baseline | 1 | 8 | 252.919 | 809.520 | 105.787 | time_limit,time_limit,time_limit | 66961612.0,66961612.0,66961612.0 |
| ii/repair | final | 1 | 8 | 240.319 | 759.581 | 100.473 | time_limit,time_limit,time_limit | 66961612.0,66961612.0,66961612.0 |
