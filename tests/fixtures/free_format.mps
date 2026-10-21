* Free-format MIP: names longer than eight characters, OBJSENSE MAXIMIZE
* on the header line, integer markers, RHS and BOUNDS entries with and
* without a set name, and an objective constant (RHS -3 means offset +3).
NAME free_example
OBJSENSE MAXIMIZE
ROWS
 N  profit
 L  capacity_limit
 G  minimum_alpha
 E  balance_beta_gamma
COLUMNS
 int_start  'MARKER'  'INTORG'
 units_alpha  profit  5  capacity_limit  2
 units_alpha  minimum_alpha  1
 units_beta  profit  3  capacity_limit  1
 units_beta  balance_beta_gamma  1
 int_end  'MARKER'  'INTEND'

 slack_gamma  profit  -1  balance_beta_gamma  -1
RHS
 profit  -3
 rhs_set  capacity_limit  9  minimum_alpha  1
 balance_beta_gamma  2
BOUNDS
 UP bounds_set units_alpha 4
 UP units_beta 5
 UP slack_gamma 10
ENDATA
