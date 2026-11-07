"""Command line: `simucraft check FILE...` and `simucraft serve`."""

import argparse
import sys

from simucraft import check
from simucraft._native import Service


def main(argv: list[str] | None = None) -> int:
    """Run the command line. Returns the exit code."""
    parser = argparse.ArgumentParser(prog="simucraft")
    commands = parser.add_subparsers(dest="command", required=True)
    files = commands.add_parser("check", help="check project files and print their diagnostics")
    files.add_argument("files", nargs="+")
    commands.add_parser("serve", help="answer JSON-RPC requests, one per line, on stdin and stdout")
    args = parser.parse_args(argv)

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
