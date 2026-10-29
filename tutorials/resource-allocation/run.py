"""Run the LP1 tutorial through the native solver's Python API."""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "_support"))
from structured import main

if __name__ == "__main__":
    main("LP1")
