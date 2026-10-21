"""Print the release version; fail if the version sources disagree.

The version lives in `[workspace.package]` of Cargo.toml, in pyproject.toml
and in `__version__` of the Python package; a release tag must equal `v`
plus this version. Runs on every supported Python (3.10+) and platform.
"""
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10: no tomllib
    tomllib = None

root = Path(__file__).resolve().parents[1]


def read(name):
    # UTF-8 explicitly: the locale encoding (cp1252 on Windows) cannot
    # decode every file.
    return (root / name).read_text(encoding="utf-8")


def table_version(name, table):
    """`version` of the TOML table `table` (dotted) in file `name`."""
    text = read(name)
    if tomllib is not None:
        data = tomllib.loads(text)
        for key in table.split("."):
            data = data[key]
        return data["version"]
    # Without tomllib: the `version = "..."` line of that table's section.
    section = re.search(rf"^\[{re.escape(table)}\][ \t]*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
    found = section and re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
    if not found:
        sys.exit(f"no version in [{table}] of {name}")
    return found.group(1)


init_file = "python/lp_ip_craft/__init__.py"
init = re.search(r'^__version__ = "([^"]+)"', read(init_file), re.M)
versions = {
    "Cargo.toml": table_version("Cargo.toml", "workspace.package"),
    "pyproject.toml": table_version("pyproject.toml", "project"),
    init_file: init.group(1) if init else None,
}
if len(set(versions.values())) != 1:
    sys.exit(f"version sources disagree: {versions}")
print(versions["Cargo.toml"])
