#!/usr/bin/env python3
"""Convert an MPS file to an ``lp-ip.model/1`` JSON model (standard library only).

    python3 scripts/mps_to_model.py model.mps -o model.json --names names.json

The output is the model artifact of the ``lp-ip.solve/1`` and
``lp-ip.evaluate/1`` job profiles. Its JSON Schema is
``crates/lp-ip-job-worker/discovery/lp-ip.model-1.schema.json``, also listed by
the worker's ``describe`` under ``input_schemas``. The command prints a JSON
summary with the SHA-256 of the written file, which a job's ``inputs`` entry
needs. ``--names`` writes the row and column names in model order, so results
can be mapped back to the MPS names. Files ending in ``.gz`` are decompressed.

Formats (``--format``):

- ``free``: whitespace-separated fields; names cannot contain spaces.
- ``fixed``: fields in columns 2-3, 5-12, 15-22, 25-36, 40-47 and 50-61;
  names may contain spaces. Text in the gaps between fields or beyond column
  61 is an error, so a misaligned file is not misread.
- ``auto`` (default): free, and fixed if free fails. If both fail, the error
  of the reading that got further is reported.

Sections: ``NAME``, ``OBJSENSE`` (``MIN``/``MINIMIZE``/``MAX``/``MAXIMIZE`` on
the next line or on the header line), ``ROWS``, ``COLUMNS``, ``RHS``,
``RANGES``, ``BOUNDS`` and ``ENDATA``, which is required. Lines starting with
``*`` and blank lines are ignored.

Conventions:

- ROWS: types ``N``, ``L``, ``G`` and ``E``. The first ``N`` row is the
  objective. Further ``N`` rows are dropped with all their entries (the names
  file lists them as ``dropped_rows``). Rows and columns keep MPS order.
- Objective sense: as given by OBJSENSE (default minimize). Coefficients keep
  their signs; the model stores the sense instead of negating them.
- Objective constant: an RHS entry ``b`` on the objective row gives
  ``objective_offset = -b`` (the convention documented for CPLEX and Gurobi:
  the objective row reads ``c x - b``).
- RHS: a row without an entry has right-hand side 0. ``L`` gives
  ``(-inf, b]``, ``G`` ``[b, +inf)`` and ``E`` ``[b, b]``.
- RANGES ``R`` (standard MPS semantics): ``L`` gives ``[b - |R|, b]``, ``G``
  ``[b, b + |R|]``, ``E`` with ``R > 0`` gives ``[b, b + R]`` and with
  ``R < 0`` ``[b + R, b]``; ``R = 0`` keeps ``[b, b]``. Ranges on dropped
  ``N`` rows are ignored; on the objective row they are an error.
- Default column bounds are ``[0, +inf)``, also for integer columns between
  ``'MARKER' 'INTORG'`` and ``'MARKER' 'INTEND'``, unless BOUNDS gives others.
- BOUNDS: ``UP`` upper, ``LO`` lower, ``FX`` both, ``FR`` free, ``MI`` lower
  ``-inf`` (the upper bound is kept), ``PL`` upper ``+inf``, ``BV`` binary
  ``[0, 1]`` (a value field is ignored), ``LI``/``UI`` integer lower/upper.
  ``UP`` or ``UI`` with a negative value on a column whose lower bound was not
  given (still the default 0) also sets the lower bound to ``-inf``, with a
  warning: the convention documented for CPLEX. An explicit lower bound is kept,
  so ``LO 0`` followed by ``UP -1`` is an error.
- Infinity: ``inf``/``infinity`` and every value of magnitude at least
  ``--infinity`` (default 1e30) mean an infinite bound, right-hand side or
  range. Objective and matrix coefficients must be finite.
- Only one RHS, RANGES and BOUNDS set is supported; a second set name is an
  error.

Errors (``MpsError``, with the line number): unsupported sections
(``QUADOBJ``, ``QSECTION``, ``QMATRIX``, ``QCMATRIX``, ``CSECTION``, ``SOS``,
``OBJNAME``, ...), ``SC`` bounds, unknown bound types, rows or columns, values
that are not numbers or NaN, an infinite coefficient or objective constant,
an infinity on the wrong side (a lower bound or ``G`` row side of +infinity,
an upper bound or ``L`` row side of -infinity, an infinite ``FX`` value or
``E`` right-hand side), duplicate rows, duplicate COLUMNS entries for one
column and row, a column whose entries are not contiguous, duplicate RHS,
RANGES or bound entries, unbalanced markers, lower above upper, a ``BV``
column with bounds outside ``[0, 1]`` and a missing ``ENDATA``.

The matrix is written as ``lp-ip.model/1`` expects it: zero-based CSR by
rows (``indptr`` has one entry per row plus one), so the column-wise COLUMNS
entries are transposed; within each row the column indices increase.
Explicit zero coefficients are kept. Infinite bounds are ``null``.

The model is written as given. The native solver still needs finite bounds on
every MIP variable after root bound inference; a MIP outside that domain is
reported as ``termination: "unsupported"`` (``NotImplementedError`` in
Python).
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import sys
from pathlib import Path

SCHEMA_VERSION = "lp-ip.model/1"
DEFAULT_INFINITY = 1e30
_FIXED_FIELDS = ((1, 3), (4, 12), (14, 22), (24, 36), (39, 47), (49, 61))
_FIXED_GAPS = ((3, 4), (12, 14), (22, 24), (36, 39), (47, 49))
_DATA_SECTIONS = ("ROWS", "COLUMNS", "RHS", "RANGES", "BOUNDS")
_UNSUPPORTED = {
    "QUADOBJ": "quadratic objectives (QUADOBJ) are not supported",
    "QSECTION": "quadratic objectives (QSECTION) are not supported",
    "QMATRIX": "quadratic objectives (QMATRIX) are not supported",
    "QCMATRIX": "quadratic constraints (QCMATRIX) are not supported",
    "CSECTION": "conic constraints (CSECTION) are not supported",
    "SOS": "special ordered sets (SOS) are not supported",
    "INDICATORS": "indicator constraints are not supported",
    "GENCONS": "general constraints (GENCONS) are not supported",
    "PWLOBJ": "piecewise-linear objectives (PWLOBJ) are not supported",
    "OBJNAME": "OBJNAME is not supported; the first N row is the objective",
}
_SENSES = {"MIN": "minimize", "MINIMIZE": "minimize", "MAX": "maximize", "MAXIMIZE": "maximize"}
_VALUE_BOUNDS = {"UP", "LO", "FX", "LI", "UI", "SC"}
_FLAG_BOUNDS = {"FR", "MI", "PL", "BV"}


class MpsError(ValueError):
    """An MPS input that cannot be converted; ``line`` is 1-based or None."""

    def __init__(self, line: int | None, message: str, fmt: str | None = None):
        self.line, self.message, self.format = line, message, fmt
        where = f"line {line}: " if line else ""
        suffix = f" (read as {fmt} MPS)" if fmt else ""
        super().__init__(f"{where}{message}{suffix}")


class _Reader:
    """One pass over the lines in one format."""

    def __init__(self, fixed: bool, infinity: float):
        self.fixed, self.infinity = fixed, infinity
        self.name = ""
        self.sense = "minimize"
        self.objective_row: str | None = None
        self.row_types: dict[str, str] = {}
        self.row_index: dict[str, int] = {}
        self.rows: list[str] = []
        self.dropped: list[str] = []
        self.columns: list[str] = []
        self.column_index: dict[str, int] = {}
        self.column_line: list[int] = []
        self.column_rows: list[dict[str, int]] = []
        self.objective: list[float] = []
        self.entries: list[list[tuple[int, float]]] = []
        self.kinds: list[str] = []
        self.lower: list[float] = []
        self.upper: list[float] = []
        self.lower_given: list[bool] = []
        self.bound_line: list[int | None] = []
        self.bounds_seen: dict[tuple[str, int], int] = {}
        self.rhs: dict[str, tuple[float, int]] = {}
        self.ranges: dict[str, tuple[float, int]] = {}
        self.set_names: dict[str, str] = {}
        self.sections: set[str] = set()
        self.current: str | None = None
        self.integer = False
        self.marker_line = 0
        self.warnings: list[str] = []

    # Lines and fields.

    def read(self, lines) -> None:
        handlers = {"ROWS": self.row_entry, "COLUMNS": self.column_entry, "RHS": self.rhs_entry,
                    "RANGES": self.range_entry, "BOUNDS": self.bound_entry}
        section = None
        for number, raw in enumerate(lines, 1):
            line = raw.rstrip("\r\n")
            if not line.strip() or line.startswith("*"):
                continue
            if not line[0].isspace():
                section = self.header(number, line)
                if section == "ENDATA":
                    self.leave(number, "ENDATA")
                    return
                continue
            if section is None:
                raise MpsError(number, "data line before the first section")
            if section in ("NAME", "OBJSENSE-DONE"):
                raise MpsError(number, f"unexpected data line after {section.split('-')[0]}")
            if section == "OBJSENSE":
                self.objsense(number, line.split())
                section = "OBJSENSE-DONE"
                continue
            handlers[section](number, line)
        raise MpsError(None, "missing ENDATA")

    def header(self, number: int, line: str) -> str:
        tokens = line.split()
        keyword = tokens[0].upper()
        if keyword in _UNSUPPORTED:
            raise MpsError(number, _UNSUPPORTED[keyword])
        if keyword == "NAME":
            self.name = line[4:].strip()
            return "NAME"
        if keyword == "OBJSENSE":
            if len(tokens) > 1:
                self.objsense(number, tokens[1:])
                return "OBJSENSE-DONE"
            return "OBJSENSE"
        if keyword == "ENDATA":
            return "ENDATA"
        if keyword not in _DATA_SECTIONS:
            raise MpsError(number, f"unknown section {tokens[0]!r}")
        if keyword in self.sections:
            raise MpsError(number, f"section {keyword} appears twice")
        needs = "ROWS" if keyword == "COLUMNS" else "COLUMNS" if keyword != "ROWS" else None
        if needs and needs not in self.sections:
            raise MpsError(number, f"section {keyword} before {needs}")
        self.leave(number, keyword)
        self.sections.add(keyword)
        return keyword

    def leave(self, number: int, next_section: str) -> None:
        """Checks that the integer markers are balanced at the end of COLUMNS."""
        if self.integer:
            raise MpsError(number, f"{next_section} inside integer markers: "
                           f"'INTORG' at line {self.marker_line} has no 'INTEND'")

    def fields(self, number: int, line: str, typed: bool = False) -> list[str]:
        """The fields of a data line. In fixed format, the type field (columns
        2-3) is kept only for ROWS and BOUNDS (``typed``) and must be empty
        otherwise, so both formats give the same list."""
        if not self.fixed:
            return line.split()
        for a, b in _FIXED_GAPS:
            if line[a:b].strip():
                raise MpsError(number, f"text in columns {a + 1}-{b} between fixed MPS fields (misaligned line)")
        if line[61:].strip():
            raise MpsError(number, "text beyond column 61 of a fixed MPS line")
        fields = [line[a:b].strip() for a, b in _FIXED_FIELDS]
        if not typed:
            if fields[0]:
                raise MpsError(number, f"unexpected {fields[0]!r} in columns 2-3")
            fields = fields[1:]
        while fields and not fields[-1]:
            fields.pop()
        return fields

    def value(self, number: int, text: str, what: str) -> float:
        try:
            v = float(text)
        except ValueError:
            raise MpsError(number, f"{what}: {text!r} is not a number") from None
        if math.isnan(v):
            raise MpsError(number, f"{what} is NaN")
        return math.copysign(math.inf, v) if abs(v) >= self.infinity else v

    def pairs(self, number: int, fields: list[str], what: str) -> list[tuple[str, str]]:
        if len(fields) not in (2, 4) or not all(fields):
            raise MpsError(number, f"{what} needs one or two (row, value) pairs")
        return [(fields[0], fields[1])] + ([(fields[2], fields[3])] if len(fields) == 4 else [])

    def known_row(self, number: int, row: str) -> str:
        kind = self.row_types.get(row)
        if kind is None:
            raise MpsError(number, f"unknown row {row!r}")
        return kind

    def check_set(self, number: int, section: str, name: str) -> None:
        if not name:
            return
        first = self.set_names.setdefault(section, name)
        if first != name:
            raise MpsError(number, f"a second {section} set {name!r} (the first is {first!r}); "
                           "only one set is supported")

    # Sections.

    def objsense(self, number: int, tokens: list[str]) -> None:
        if len(tokens) != 1 or tokens[0].upper() not in _SENSES:
            raise MpsError(number, f"OBJSENSE needs MIN, MINIMIZE, MAX or MAXIMIZE, got {' '.join(tokens)!r}")
        self.sense = _SENSES[tokens[0].upper()]

    def row_entry(self, number: int, line: str) -> None:
        fields = self.fields(number, line, typed=True)
        if len(fields) != 2 or not all(fields):
            raise MpsError(number, "a ROWS entry needs a type and a row name")
        kind, name = fields[0].upper(), fields[1]
        if kind not in ("N", "L", "G", "E"):
            raise MpsError(number, f"unknown row type {fields[0]!r} (N, L, G or E)")
        if name in self.row_types:
            raise MpsError(number, f"duplicate row {name!r}")
        self.row_types[name] = kind
        if kind != "N":
            self.row_index[name] = len(self.rows)
            self.rows.append(name)
        elif self.objective_row is None:
            self.objective_row = name
        else:
            self.dropped.append(name)

    def column_entry(self, number: int, line: str) -> None:
        tokens = line.split()
        if "'MARKER'" in tokens:
            self.marker(number, tokens)
            return
        fields = self.fields(number, line)
        if len(fields) < 3 or not fields[0]:
            raise MpsError(number, "a COLUMNS entry needs a column and one or two (row, value) pairs")
        column = fields[0]
        j = self.column_index.get(column)
        if j is None:
            j = self.column_index[column] = len(self.columns)
            self.columns.append(column)
            self.column_line.append(number)
            self.column_rows.append({})
            self.objective.append(0.0)
            self.entries.append([])
            self.kinds.append("integer" if self.integer else "continuous")
            self.lower.append(0.0)
            self.upper.append(math.inf)
            self.lower_given.append(False)
            self.bound_line.append(None)
            self.current = column
        elif column != self.current:
            raise MpsError(number, f"column {column!r} appears again; its entries must be contiguous "
                           f"(first at line {self.column_line[j]})")
        for row, text in self.pairs(number, fields[1:], "a COLUMNS entry"):
            kind = self.known_row(number, row)
            if row in self.column_rows[j]:
                raise MpsError(number, f"duplicate entry for column {column!r} in row {row!r} "
                               f"(first at line {self.column_rows[j][row]})")
            self.column_rows[j][row] = number
            v = self.value(number, text, f"coefficient of {column!r} in {row!r}")
            if math.isinf(v):
                raise MpsError(number, f"coefficient of {column!r} in {row!r} is infinite")
            if kind != "N":
                self.entries[j].append((self.row_index[row], v))
            elif row == self.objective_row:
                self.objective[j] = v

    def marker(self, number: int, tokens: list[str]) -> None:
        at = tokens.index("'MARKER'")
        kind = tokens[at + 1] if at + 1 < len(tokens) else ""
        if kind == "'INTORG'":
            if self.integer:
                raise MpsError(number, f"'INTORG' inside integer markers (opened at line {self.marker_line})")
            self.integer, self.marker_line = True, number
        elif kind == "'INTEND'":
            if not self.integer:
                raise MpsError(number, "'INTEND' without 'INTORG'")
            self.integer = False
        else:
            raise MpsError(number, f"unknown marker {kind!r} ('INTORG' or 'INTEND')")
        self.current = None

    def side_entries(self, number: int, line: str, section: str, store: dict) -> None:
        fields = self.fields(number, line)
        if self.fixed:
            name, rest = (fields[0] if fields else ""), fields[1:]
        else:
            name, rest = (fields[0], fields[1:]) if len(fields) % 2 else ("", fields)
        self.check_set(number, section, name)
        for row, text in self.pairs(number, rest, f"a {section} entry"):
            kind = self.known_row(number, row)
            if row in store:
                raise MpsError(number, f"duplicate {section} entry for row {row!r} (first at line {store[row][1]})")
            v = self.value(number, text, f"{section} of row {row!r}")
            if section == "RANGES" and kind == "N":
                if row == self.objective_row:
                    raise MpsError(number, f"RANGES on the objective row {row!r}")
                continue
            store[row] = (v, number)

    def rhs_entry(self, number: int, line: str) -> None:
        self.side_entries(number, line, "RHS", self.rhs)

    def range_entry(self, number: int, line: str) -> None:
        self.side_entries(number, line, "RANGES", self.ranges)

    def bound_entry(self, number: int, line: str) -> None:
        fields = self.fields(number, line, typed=True)
        if not fields or not fields[0]:
            raise MpsError(number, "a BOUNDS entry needs a type")
        kind = fields[0].upper()
        if kind == "SC":
            raise MpsError(number, "semi-continuous (SC) bounds are not supported")
        if kind not in _VALUE_BOUNDS | _FLAG_BOUNDS:
            raise MpsError(number, f"unknown bound type {fields[0]!r}")
        if self.fixed:
            fields += [""] * (4 - len(fields))
            name, column, text = fields[1], fields[2], fields[3] or None
        else:
            name, column, text = self.free_bound(number, kind, fields)
        self.check_set(number, "BOUNDS", name)
        j = self.column_index.get(column)
        if j is None:
            raise MpsError(number, f"unknown column {column!r}")
        if (kind, j) in self.bounds_seen:
            raise MpsError(number, f"duplicate {kind} bound for column {column!r} "
                           f"(first at line {self.bounds_seen[kind, j]})")
        self.bounds_seen[kind, j] = number
        self.bound_line[j] = number
        if kind in _FLAG_BOUNDS:
            self.flag_bound(kind, j)
            return
        if text is None:
            raise MpsError(number, f"{kind} bound for column {column!r} needs a value")
        v = self.value(number, text, f"{kind} bound of {column!r}")
        if kind in ("LO", "LI", "FX") and v == math.inf:
            raise MpsError(number, f"{kind} bound +infinity for column {column!r}: a lower bound "
                           "of +infinity leaves no value (use PL for an unbounded upper side)")
        if kind in ("UP", "UI", "FX") and v == -math.inf:
            raise MpsError(number, f"{kind} bound -infinity for column {column!r}: an upper bound "
                           "of -infinity leaves no value (use MI for an unbounded lower side)")
        if kind in ("LI", "UI") and self.kinds[j] == "continuous":
            self.kinds[j] = "integer"
        if kind in ("LO", "LI", "FX"):
            self.lower[j], self.lower_given[j] = v, True
        if kind in ("UP", "UI", "FX"):
            if kind != "FX" and v < 0 and not self.lower_given[j]:
                self.lower[j] = -math.inf
                self.warnings.append(f"line {number}: {kind} bound {v} on column {column!r} without "
                                     "a lower bound: lower bound set to -infinity")
            self.upper[j] = v

    def free_bound(self, number: int, kind: str, fields: list[str]) -> tuple[str, str, str | None]:
        """Set name, column and value of a free BOUNDS entry; the set name may be missing."""
        n = len(fields)
        if kind in _VALUE_BOUNDS:
            if n == 4:
                return fields[1], fields[2], fields[3]
            if n == 3:
                if fields[1] not in self.column_index and fields[2] in self.column_index:
                    raise MpsError(number, f"{kind} bound for column {fields[2]!r} needs a value")
                return "", fields[1], fields[2]
        else:
            if n == 2:
                return "", fields[1], None
            if n == 3:
                if fields[2] in self.column_index or fields[1] not in self.column_index:
                    return fields[1], fields[2], None
                return "", fields[1], None
            if n == 4:
                return fields[1], fields[2], None
        needs = "a value" if kind in _VALUE_BOUNDS else "no value"
        raise MpsError(number, f"a {kind} entry needs an optional set name, a column and {needs}")

    def flag_bound(self, kind: str, j: int) -> None:
        if kind == "FR":
            self.lower[j], self.upper[j] = -math.inf, math.inf
        elif kind == "MI":
            self.lower[j] = -math.inf
        elif kind == "PL":
            self.upper[j] = math.inf
        else:  # BV
            self.kinds[j] = "binary"
            self.lower[j], self.upper[j] = 0.0, 1.0
        if kind != "PL":
            self.lower_given[j] = True

    # The model.

    def model(self) -> tuple[dict, dict]:
        if not self.columns:
            raise MpsError(None, "the model has no columns")
        offset = 0.0
        if self.objective_row in self.rhs:
            v, line = self.rhs[self.objective_row]
            if math.isinf(v):
                raise MpsError(line, f"the objective constant (RHS of {self.objective_row!r}) is infinite")
            offset = -v if v else 0.0
        row_lower, row_upper = [], []
        for row in self.rows:
            kind = self.row_types[row]
            b, line = self.rhs.get(row, (0.0, None))
            if kind == "E" and math.isinf(b):
                raise MpsError(line, f"E row {row!r} has an infinite right-hand side")
            if kind == "L" and b == -math.inf or kind == "G" and b == math.inf:
                raise MpsError(line, f"{kind} row {row!r} has right-hand side {b}, which leaves no value")
            lo, up = {"L": (-math.inf, b), "G": (b, math.inf), "E": (b, b)}[kind]
            if row in self.ranges:
                r, line = self.ranges[row]
                if kind == "L":
                    lo = b - abs(r)
                elif kind == "G":
                    up = b + abs(r)
                elif r > 0:
                    up = b + r
                elif r < 0:
                    lo = b + r
            if not (lo <= up and lo != math.inf and up != -math.inf):
                raise MpsError(line, f"row {row!r} has bounds [{lo}, {up}]")
            row_lower.append(lo)
            row_upper.append(up)
        for j, column in enumerate(self.columns):
            lo, up, line = self.lower[j], self.upper[j], self.bound_line[j]
            if lo > up:
                raise MpsError(line, f"column {column!r} has lower bound {lo} above upper bound {up}")
            if self.kinds[j] == "binary" and (lo < 0 or up > 1):
                raise MpsError(line, f"binary (BV) column {column!r} has bounds [{lo}, {up}] outside [0, 1]")
        # COLUMNS is column-wise; lp-ip.model/1 wants CSR by rows. Columns are
        # visited in order, so the indices of each row increase.
        counts = [0] * len(self.rows)
        for entries in self.entries:
            for i, _ in entries:
                counts[i] += 1
        indptr = [0]
        for c in counts:
            indptr.append(indptr[-1] + c)
        nnz = indptr[-1]
        indices, values, fill = [0] * nnz, [0.0] * nnz, indptr[:-1]
        for j, entries in enumerate(self.entries):
            for i, v in entries:
                indices[fill[i]], values[fill[i]] = j, v
                fill[i] += 1

        def side(values):
            return [None if math.isinf(v) else v for v in values]

        model = {
            "schema_version": SCHEMA_VERSION,
            "sense": self.sense,
            "objective": self.objective,
            "objective_offset": offset,
            "variable_lower": side(self.lower),
            "variable_upper": side(self.upper),
            "kinds": self.kinds,
            "matrix": {"indptr": indptr, "indices": indices, "values": values},
            "row_lower": side(row_lower),
            "row_upper": side(row_upper),
        }
        names = {"problem": self.name, "objective_row": self.objective_row, "rows": self.rows,
                 "columns": self.columns, "dropped_rows": self.dropped, "warnings": self.warnings}
        return model, names


def parse_mps(text: str, format: str = "auto", infinity: float = DEFAULT_INFINITY) -> tuple[dict, dict]:
    """Returns the ``lp-ip.model/1`` model and the names of an MPS text.

    The names dict holds ``problem``, ``objective_row``, ``rows`` and
    ``columns`` in model order, ``dropped_rows`` (further N rows) and
    ``warnings``. Raises ``MpsError`` for input it cannot convert.
    """
    if format not in ("auto", "free", "fixed"):
        raise ValueError(f"format must be auto, free or fixed, got {format!r}")
    if not infinity > 0:
        raise ValueError("infinity must be positive")
    lines = text.splitlines()
    errors = []
    for fixed in ([False, True] if format == "auto" else [format == "fixed"]):
        reader = _Reader(fixed, infinity)
        try:
            reader.read(lines)
            return reader.model()
        except MpsError as e:
            errors.append(MpsError(e.line, e.message, ("fixed" if fixed else "free") if format == "auto" else None))
    # The reading that got further; a missing ENDATA or a model error comes last.
    raise max(errors, key=lambda e: math.inf if e.line is None else e.line)


def read_text(path: Path) -> str:
    data = gzip.decompress(path.read_bytes()) if path.suffix == ".gz" else path.read_bytes()
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("latin-1")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                     epilog="See the module docstring for the conventions.")
    parser.add_argument("input", type=Path, help="MPS file (.mps, or .mps.gz)")
    parser.add_argument("-o", "--output", type=Path, required=True, help="lp-ip.model/1 JSON file to write")
    parser.add_argument("--names", type=Path, help="also write the row and column names in model order")
    parser.add_argument("--format", choices=["auto", "free", "fixed"], default="auto")
    parser.add_argument("--infinity", type=float, default=DEFAULT_INFINITY,
                        help="magnitude from which bounds, RHS and ranges are infinite (default 1e30)")
    parser.add_argument("--indent", type=int, help="indent the JSON (default: compact)")
    args = parser.parse_args(argv)
    try:
        model, names = parse_mps(read_text(args.input), args.format, args.infinity)
    except MpsError as e:
        print(f"{args.input}: {e}", file=sys.stderr)
        return 1
    separators = None if args.indent is not None else (",", ":")
    data = (json.dumps(model, indent=args.indent, separators=separators, allow_nan=False) + "\n").encode()
    args.output.write_bytes(data)
    if args.names:
        args.names.write_text(json.dumps(names, indent=2) + "\n", encoding="utf-8")
    for warning in names["warnings"]:
        print(f"{args.input}: warning: {warning}", file=sys.stderr)
    print(json.dumps({
        "model": str(args.output), "sha256": hashlib.sha256(data).hexdigest(), "format": SCHEMA_VERSION,
        "sense": model["sense"], "variables": len(model["objective"]), "rows": len(model["row_lower"]),
        "nonzeros": len(model["matrix"]["values"]),
        "integer_variables": sum(k != "continuous" for k in model["kinds"]),
        "names": None if args.names is None else str(args.names), "warnings": len(names["warnings"]),
    }, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
