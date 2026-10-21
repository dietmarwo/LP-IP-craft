* Maximization: OBJSENSE with MAX on the next line, and an objective
* constant (RHS 10 on the objective row means offset -10).
NAME          MAXLP
OBJSENSE
    MAX
ROWS
 N  PROFIT
 L  R1
 L  R2
COLUMNS
    X         PROFIT             3.0   R1                 1.0
    X         R2                 1.0
    Y         PROFIT             2.0   R1                 1.0
    Y         R2                 3.0
RHS
    RHS       PROFIT            10.0   R1                 4.0
    RHS       R2                 6.0
BOUNDS
 UP BND       X                  3.0
ENDATA
