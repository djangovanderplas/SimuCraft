"""WP1.4: the PID example is built and saved from Python."""

import json
import subprocess
import sys
import tomllib

import pytest

import simucraft as sc
from simucraft.cli import main


def build_pid(path):
    proj = sc.create(path, "pid")
    m = proj.model()
    m.set_param("Kp", 2.0, tunable=True)
    m.set_param("Ki", expr="Kp * 1.5")
    with proj.transaction("Build the PID loop"):
        ref = m.add_block("core.Constant", name="ref", params={"value": 1.0})
        err = m.add_block("core.Sum", name="error", params={"signs": "+-"})
        pid = m.add_block("core.PID", name="pid", params={"P": "Kp", "I": "Ki", "D": 0.0})
        plant = m.add_block("core.TransferFcn", name="plant", params={"num": [1.0], "den": [0.5, 1.0]})
        y = m.add_block("core.Outport", name="y")
        m.connect("ref", err.u1)
        m.connect(plant.y, err.u2)
        m.connect(err.y, pid.u)
        m.connect("pid.y", plant.u)
        m.connect(plant.y, y.u)
    return proj, m, pid, ref


def test_pid_example_is_built_and_saved(tmp_path):
    proj, m, pid, _ = build_pid(tmp_path)
    assert proj.status()["unsaved"] == ["models/pid.scmodel"]
    assert proj.save() == ["models/pid.scmodel"]

    path = tmp_path / "models" / "pid.scmodel"
    assert sc.check(path) == []
    data = tomllib.loads(path.read_text())
    blocks = {b["name"]: b for b in data["blocks"].values()}
    assert blocks["plant"]["inputs"]["u"] == pid.y
    assert data["parameters"]["Ki"]["expr"] == "Kp * 1.5"

    again = sc.open(tmp_path).model()
    assert again.block("pid").id == pid.id


def test_a_failed_transaction_rolls_back(tmp_path):
    proj, m, _, ref = build_pid(tmp_path)
    with pytest.raises(sc.CommandError) as e:
        with proj.transaction("Bad"):
            m.add_block("core.Gain", name="k")
            m.add_block("core.Gain", name="k")  # duplicate name
    assert e.value.diagnostics[0]["code"] == "F007"
    assert "k" not in {b["name"] for b in m.get()["blocks"].values()}
    ref.rename("setpoint")
    proj.undo()
    assert m.block("ref").id == ref.id


def test_cli_check_and_serve(tmp_path, capsys):
    proj, *_ = build_pid(tmp_path)
    proj.save()
    assert main(["check", str(tmp_path / "models" / "pid.scmodel")]) == 0
    bad = tmp_path / "bad.scmodel"
    bad.write_text('format = "simucraft.model/0.1"\nid = "m_short"\nname = "x"\n')
    assert main(["check", str(bad)]) == 1
    assert "F006" in capsys.readouterr().out

    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "project.open", "params": {"path": str(tmp_path)}},
        {"jsonrpc": "2.0", "id": 2, "method": "find", "params": {"query": "pid"}},
    ]
    out = subprocess.run(
        [sys.executable, "-m", "simucraft", "serve"],
        input="\n".join(json.dumps(r) for r in requests) + "\n",
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()
    assert [json.loads(line)["id"] for line in out] == [1, 2]
    assert json.loads(out[1])["result"][0]["name"] == "pid"
