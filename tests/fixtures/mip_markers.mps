* MIP: integer markers (A, B), a continuous column (C), BV (D) and
* LI/UI integer bounds (E).
NAME          MARKMIP
ROWS
 N  OBJ
 L  CAP
 L  PAIR
 G  NEED
COLUMNS
    MARKER    'MARKER'                 'INTORG'
    A         OBJ               -5.0   CAP                2.0
    A         PAIR               1.0
    B         OBJ               -4.0   CAP                3.0
    B         PAIR               1.0
    MARKER    'MARKER'                 'INTEND'
    C         OBJ               -3.0   CAP                1.0
    D         OBJ               -8.0   CAP                4.0
    D         NEED               1.0
    E         OBJ               -1.0   CAP                1.0
    E         NEED               1.0
RHS
    RHS       CAP               12.0   PAIR               4.0
    RHS       NEED               2.0
BOUNDS
 UP BND       A                  3.0
 UP BND       C                  2.5
 BV BND       D
 LI BND       E                  1.0
 UI BND       E                  4.0
ENDATA
