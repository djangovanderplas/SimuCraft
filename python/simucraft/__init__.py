"""SimuCraft: open-source block-diagram modeling and simulation."""

import json
from os import PathLike

from simucraft._native import __version__
from simucraft._native import check_file as _check_file

__all__ = ["__version__", "check"]


def check(path: str | PathLike[str]) -> list[dict]:
    """Check one project file. Returns its diagnostics; an empty list means valid."""
    return json.loads(_check_file(str(path)))
