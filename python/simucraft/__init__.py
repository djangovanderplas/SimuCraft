"""SimuCraft: open-source block-diagram modeling and simulation."""

import itertools
import json
from contextlib import contextmanager
from dataclasses import dataclass
from os import PathLike

from simucraft._native import Service as _Service
from simucraft._native import __version__
from simucraft._native import check_file as _check_file

__all__ = ["Block", "CommandError", "Model", "Project", "Run", "Signal", "__version__", "check", "create", "open"]


def check(path: str | PathLike[str]) -> list[dict]:
    """Check one project file. Returns its diagnostics; an empty list means valid."""
    return json.loads(_check_file(str(path)))


class CommandError(Exception):
    """A Model API command failed. `diagnostics` holds the structured reasons."""

    def __init__(self, message: str, diagnostics: list[dict]):
        super().__init__(message)
        self.diagnostics = diagnostics


class _Client:
    """Sends Model API commands to an in-process model service."""

    def __init__(self):
        self._service = _Service()
        self._ids = itertools.count(1)

    def call(self, method: str, **params):
        request = {"jsonrpc": "2.0", "id": next(self._ids), "method": method, "params": params}
        response = json.loads(self._service.handle(json.dumps(request)))
        if "error" in response:
            error = response["error"]
            raise CommandError(error["message"], error.get("diagnostics", []))
        return response["result"]


def open(path: str | PathLike[str]) -> "Project":
    """Open the project in folder `path`."""
    client = _Client()
    return Project(client, client.call("project.open", path=str(path)))


def create(path: str | PathLike[str], name: str) -> "Project":
    """Make a new project in folder `path`, with one empty model `models/<name>.scmodel`."""
    client = _Client()
    return Project(client, client.call("project.create", path=str(path), name=name))


class Project:
    """An open project. Edits stay in memory until `save`."""

    def __init__(self, client: _Client, info: dict):
        self._client = client
        self.root = info["root"]
        self.top = info["top"]

    def model(self, path: str | None = None) -> "Model":
        """A model file of the project, by path from the project root. The default is the top model."""
        return Model(self, path or self.top)

    @contextmanager
    def transaction(self, label: str):
        """Group edits into one undo step. An exception rolls them all back."""
        self._client.call("tx.begin", label=label)
        try:
            yield
        except BaseException:
            self._client.call("tx.rollback")
            raise
        self._client.call("tx.commit")

    def undo(self) -> None:
        """Undo the last edit or transaction."""
        self._client.call("undo")

    def redo(self) -> None:
        """Redo the last undone edit or transaction."""
        self._client.call("redo")

    def status(self) -> dict:
        """Unsaved files and their diagnostics."""
        return self._client.call("project.status")

    def save(self) -> list[str]:
        """Write the changed files. Returns their paths."""
        return self._client.call("project.save")["written"]


class Model:
    """One model file of a project."""

    def __init__(self, project: Project, path: str):
        self.project = project
        self.path = path

    def _call(self, method: str, **params):
        return self.project._client.call(method, **params)

    def add_block(self, type: str, name: str, params: dict | None = None, inputs: dict | None = None) -> "Block":
        """Add a block. `inputs` maps input ports to sources (`"pid.y"`)."""
        result = self._call("block.add", model=self.path, type=type, name=name, params=params or {}, inputs=inputs or {})
        return Block(self, result["created"][0], name)

    def block(self, name: str) -> "Block":
        """An existing block, by name or ID."""
        found = [b for b in self._call("find", query=name) if name in (b["name"], b["id"]) and b["file"] == self.path]
        if not found:
            raise CommandError(f"no block `{name}` in {self.path}", [])
        return Block(self, found[0]["id"], found[0]["name"])

    def connect(self, source: str, destination: str) -> list[dict]:
        """Set the source of an input; it replaces the old source. A bare block name as source means its `y`."""
        if "." not in source:
            source += ".y"
        return self._call("connect", source=source, destination=destination)["diff"]

    def disconnect(self, destination: str) -> list[dict]:
        """Remove the source of an input."""
        return self._call("disconnect", destination=destination)["diff"]

    def set_param(self, name: str, value=None, *, expr: str | None = None, unit: str | None = None,
                  tunable: bool | None = None) -> list[dict]:
        """Set a model parameter to a literal `value` or an expression `expr`."""
        args = {"value": value, "expr": expr, "unit": unit, "tunable": tunable}
        return self._call("param.set", model=self.path, name=name, **{k: v for k, v in args.items() if v is not None})

    def get(self) -> dict:
        """The model's data, as in the file."""
        return self._call("model.get", path=self.path)["model"]

    def compile(self) -> list[dict]:
        """Compile the model, with its unsaved edits. Returns the diagnostics; an empty list means clean."""
        return self._call("compile", model=self.path)["diagnostics"]

    def simulate(self, stop_time: float | None = None, signals: list[str] | None = None) -> "Run":
        """Compile and run the model. The solver settings come from its `[simulation]` table.

        `signals` names the outports and logged signals to record; the default is all of them.
        """
        result = self._call("simulate", model=self.path, stop_time=stop_time, signals=signals or [])
        return Run(self.project, result["run"], result["signals"])


@dataclass
class Signal:
    """One recorded signal. A signal of width `w` has `w` values per time, one row after the other."""

    name: str
    t: list[float]
    values: list[float]
    width: int


class Run:
    """A finished simulation run. Its traces stay in the model service."""

    def __init__(self, project: Project, id: str, signals: list[str]):
        self.project = project
        self.id = id
        self.signals = signals

    def signal(self, name: str, decimated: bool = False) -> Signal:
        """All samples of a signal, or at most 1000 of them if `decimated`."""
        mode = "decimated" if decimated else "full"
        r = self.project._client.call("signal.get", run=self.id, signal=name, mode=mode)
        return Signal(name, r["t"], r["values"], r["width"])

    def summary(self, name: str) -> dict:
        """Statistics of a signal: min, max, mean and final value, with their times."""
        return self.project._client.call("signal.get", run=self.id, signal=name, mode="summary")

    def __repr__(self) -> str:
        return f"Run({self.id!r}, signals={self.signals!r})"


class Block:
    """A block in a model. An attribute that is not defined here is a port: `pid.y` is `"<id>.y"`."""

    def __init__(self, model: Model, id: str, name: str):
        self.model = model
        self.id = id
        self.name = name

    def __getattr__(self, port: str) -> str:
        if port.startswith("_"):
            raise AttributeError(port)
        return f"{self.id}.{port}"

    def set_params(self, **params) -> list[dict]:
        """Set block parameters; `None` removes one."""
        return self.model._call("block.set_params", target=self.id, params=params)["diff"]

    def rename(self, name: str) -> list[dict]:
        """Give the block a new name."""
        diff = self.model._call("block.rename", target=self.id, name=name)["diff"]
        self.name = name
        return diff

    def remove(self) -> list[dict]:
        """Remove the block and the inputs that read from it."""
        return self.model._call("block.remove", target=self.id)["diff"]

    def __repr__(self) -> str:
        return f"Block({self.name!r}, {self.id!r})"
