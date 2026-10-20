"""Print the release version; fail if the version sources disagree.

The version lives in `[workspace.package]` of Cargo.toml, in pyproject.toml
and in `__version__` of the Python package; a release tag must equal `v`
plus this version.
"""
import re
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
cargo = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
project = tomllib.loads((root / "pyproject.toml").read_text())["project"]["version"]
init = re.search(r'^__version__ = "([^"]+)"', (root / "python/lp_ip_craft/__init__.py").read_text(), re.M)
versions = {"Cargo.toml": cargo, "pyproject.toml": project,
            "python/lp_ip_craft/__init__.py": init.group(1) if init else None}
if len(set(versions.values())) != 1:
    sys.exit(f"version sources disagree: {versions}")
print(cargo)
