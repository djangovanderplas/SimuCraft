"""Gate G1 for Python and the CLI: the PID with plant matches its Simulink reference.

The model is `conformance/models/pid_plant.scmodel`, the same file as in the conformance harness.
The test adds a `[simulation]` table for RK4 at step 1e-3, as the Simulink ode4 run uses.
"""

import csv
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import simucraft as sc

CONFORMANCE = Path(__file__).resolve().parents[2] / "conformance"
# The same limit as the `pid_plant` ode4 case in conformance/src/main.rs: only rounding differs.
ATOL = RTOL = 1e-12


@pytest.fixture
def project(tmp_path):
    (tmp_path / "models").mkdir()
    (tmp_path / "simucraft.toml").write_text(
        'format = "simucraft.project/0.1"\nname = "g1"\ntop = "models/pid_plant.scmodel"\n'
    )
    model = tmp_path / "models" / "pid_plant.scmodel"
    shutil.copy(CONFORMANCE / "models" / "pid_plant.scmodel", model)
    with model.open("a") as f:
        f.write('\n[simulation]\nsolver = "rk4"\nfixed_step = 0.001\nstop_time = 5.0\n')
    assert sc.check(model) == []
    return tmp_path


def reference() -> dict[str, list[float]]:
    with (CONFORMANCE / "references" / "ref_pid_plant.ode4.csv").open() as f:
        rows = list(csv.DictReader(f))
    return {k: [float(r[k]) for r in rows] for k in rows[0]}


def assert_matches(ours: dict[str, list[float]]):
    ref = reference()
    assert len(ours["time"]) == len(ref["time"])
    assert max(abs(a - b) for a, b in zip(ours["time"], ref["time"])) < 1e-12
    for name in ("y", "u"):
        worst = max(abs(a - r) / (ATOL + RTOL * abs(r)) for a, r in zip(ours[name], ref[name]))
        assert worst <= 1.0, f"{name}: worst error is {worst:.3g} times the limit"


def test_pid_plant_matches_simulink_from_python(project):
    m = sc.open(project).model()
    assert m.compile() == []
    run = m.simulate()
    assert sorted(run.signals) == ["u", "y"]
    y, u = run.signal("y"), run.signal("u")
    assert_matches({"time": y.t, "y": y.values, "u": u.values})


def test_pid_plant_matches_simulink_from_the_cli(project):
    out = subprocess.run(
        [sys.executable, "-m", "simucraft", "simulate", str(project)], capture_output=True, text=True, check=True
    ).stdout
    rows = list(csv.DictReader(out.splitlines()))
    assert_matches({k: [float(r[k]) for r in rows] for k in rows[0]})


def test_the_cli_reports_a_model_that_does_not_compile(project):
    model = project / "models" / "pid_plant.scmodel"
    model.write_text(model.read_text().replace('type = "core.Gain"', 'type = "core.Nope"').replace(
        'type = "core.PID"', 'type = "core.Nope"'))
    r = subprocess.run([sys.executable, "-m", "simucraft", "simulate", str(project)], capture_output=True, text=True)
    assert r.returncode == 1 and "C001" in r.stderr
