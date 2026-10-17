"""Parser contract checks; requires the same SciPy environment as conversion."""
import tempfile,unittest
from pathlib import Path
from prepare_public import parse
class Parser(unittest.TestCase):
    def model(self,text):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'test.mps';p.write_text(text);return parse(p)
    def test_marker_defaults_and_explicit_general_integer_bounds(self):
        m=self.model("""NAME TEST
ROWS
 N OBJ
 L R
COLUMNS
 M1 'MARKER' 'INTORG'
 X OBJ 1 R 2
 Y OBJ 3 R 4
 M2 'MARKER' 'INTEND'
 Z OBJ 2 R 1
RHS
 RHS R 5 OBJ -7
BOUNDS
 LO B Y 0
ENDATA
""")
        self.assertEqual(m['upper'],[1.,float('inf'),float('inf')])
        self.assertEqual(m['kinds'],['integer','integer','continuous'])
        self.assertEqual(m['objective_offset'],7.)
    def test_ranges_and_duplicate_column_entries(self):
        m=self.model("""NAME TEST
ROWS
 N OBJ
 E EQ
 G GE
 L LE
COLUMNS
 X EQ 2 GE 3
 X EQ -1 LE 4
RHS
 B EQ 5 GE 6
 B LE 7
RANGES
 R EQ -2 GE 3
 R LE 4
ENDATA
""")
        self.assertEqual(m['values'],[1.,3.,4.])
        self.assertEqual(m['row_lower'],[3.,6.,3.])
        self.assertEqual(m['row_upper'],[5.,9.,7.])
    def test_unsupported_sections_fail_closed(self):
        with self.assertRaises(AssertionError):self.model('NAME TEST\nQUADOBJ\n')
if __name__=='__main__':unittest.main()
