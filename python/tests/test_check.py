import tomllib
from pathlib import Path

import simucraft

EXAMPLES = Path(__file__).parents[2] / "examples"
FILES = [p for p in EXAMPLES.rglob("*") if p.is_file()]


def test_version():
    assert simucraft.__version__


def test_examples_are_valid():
    assert FILES
    for path in FILES:
        assert simucraft.check(path) == [], path


def test_examples_are_toml_1_0():
    # Python before 3.15 reads TOML 1.0 only; the files must stay readable there.
    for path in FILES:
        tomllib.loads(path.read_text())


def test_diagnostics_have_codes(tmp_path):
    bad = tmp_path / "bad.scmodel"
    bad.write_text('format = "simucraft.model/0.1"\nid = "m_short"\nname = "x"\n')
    [d] = simucraft.check(bad)
    assert d["code"] == "F006"
    assert d["severity"] == "error"
