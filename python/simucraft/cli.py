"""Command line: `simucraft check FILE...`, `simucraft simulate PROJECT` and `simucraft serve`."""

import argparse
import csv
import sys

import simucraft
from simucraft import CommandError, check
from simucraft._native import Service


def main(argv: list[str] | None = None) -> int:
    """Run the command line. Returns the exit code."""
    parser = argparse.ArgumentParser(prog="simucraft")
    commands = parser.add_subparsers(dest="command", required=True)
    files = commands.add_parser("check", help="check project files and print their diagnostics")
    files.add_argument("files", nargs="+")
    run = commands.add_parser("simulate", help="run a model and print its signals as CSV")
    run.add_argument("project", help="the project folder")
    run.add_argument("--model", help="model file from the project root; the default is the top model")
    run.add_argument("--stop-time", type=float)
    commands.add_parser("serve", help="answer JSON-RPC requests, one per line, on stdin and stdout")
    args = parser.parse_args(argv)

    if args.command == "simulate":
        return simulate(args.project, args.model, args.stop_time)

    if args.command == "check":
        failed = False
        for path in args.files:
            try:
                diagnostics = check(path)
            except (OSError, ValueError) as e:
                print(f"{path}: {e}", file=sys.stderr)
                return 2
            for d in diagnostics:
                print(f"{path}: {d['severity']} {d['code']}: {d['message']}")
                failed |= d["severity"] == "error"
        return 1 if failed else 0

    service = Service()
    for line in sys.stdin:
        if line.strip():
            print(service.handle(line), flush=True)
    return 0


def simulate(project: str, model: str | None, stop_time: float | None) -> int:
    """Print the run as CSV: `time`, then one column per signal element, with full precision."""
    try:
        r = simucraft.open(project).model(model).simulate(stop_time=stop_time)
        signals = [r.signal(name) for name in r.signals]
    except (CommandError, OSError) as e:
        print(f"{project}: {e}", file=sys.stderr)
        for d in getattr(e, "diagnostics", []):
            print(f"  {d['severity']} {d['code']}: {d['message']}", file=sys.stderr)
        return 1
    header = ["time"]
    for s in signals:
        header += [s.name] if s.width == 1 else [f"{s.name}[{i}]" for i in range(s.width)]
    out = csv.writer(sys.stdout, lineterminator="\n")
    out.writerow(header)
    for row, t in enumerate(signals[0].t if signals else []):
        values = [v for s in signals for v in s.values[row * s.width:(row + 1) * s.width]]
        out.writerow([repr(t)] + [repr(v) for v in values])
    return 0
