* LP with ranges on L, G and E rows, free and negative bounds, a second
* N row (ignored), an objective constant and a row name with a space
* (fixed format only).
NAME          RANGELP
ROWS
 N  COST
 N  ALT
 G  LIM A
 E  ROWB
 L  ROWC
 E  ROWD
 G  ROWE
COLUMNS
    X1        COST               1.0   LIM A              1.0
    X1        ROWD               1.0   ALT                1.0
    X2        COST               1.0   ROWB               1.0
    X3        COST               1.0   ROWC              -1.0
    X3        ALT                4.0
    X4        COST               2.0   ROWB               1.0
    X4        ROWE               1.0
    X5        COST              -1.0   ROWD               1.0
    X5        ROWE               1.0
RHS
    RHS       COST              -2.5   LIM A              2.0
    RHS       ROWB               1.0   ROWC               7.0
    RHS       ROWD               4.0   ALT              100.0
RANGES
    RNG       LIM A              3.0   ROWB              -4.0
    RNG       ROWC               4.0   ROWD               3.0
BOUNDS
 FR BND       X1
 MI BND       X2
 UP BND       X2                10.0
 UP BND       X3                -2.0
 LO BND       X5                -5.0
 UP BND       X5                 5.0
ENDATA
