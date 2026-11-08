# Handoff: gate G1 in review

Date: 2026-11-08. All Phase 1 work packages are done or in review. Gate G1 passes on the branches. It is not on `main` yet.

## Next action for the owner

1. Review and merge the pull requests in this order. Each later one contains the earlier ones. A trial merge of all five in this order on 2026-11-08 had no conflicts.
   1. [#8 G1: Editor stack ADR, 9 reference models, check.sh with conda](https://github.com/djangovanderplas/SimuCraft/pull/8)
   2. [#9 G1: Contract change: variadic inputs and sample time in build](https://github.com/djangovanderplas/SimuCraft/pull/9). Review this one with the most care: it changes the frozen contract.
   3. [#10 G1: compile and simulate in the service, Python and CLI](https://github.com/djangovanderplas/SimuCraft/pull/10)
   4. [#11 WP1.6: MCP server](https://github.com/djangovanderplas/SimuCraft/pull/11)
   5. [#12 WP1.7: Read-only diagram viewer](https://github.com/djangovanderplas/SimuCraft/pull/12)
   6. This handoff pull request, last.
2. Read the "Decisions" section of each pull request. Reject the ones you do not agree with.
3. After the merges, run `scripts/check.sh` on `main`. It now needs Node.js (for the editor) and finds `.venv` itself, also with conda active.

## State

`main` has WP1.1 to WP1.5 and WP1.8. The owner merged them on 2026-11-08 (#1 to #7).

| WP | Where | Done-when result |
| --- | --- | --- |
| 1.1 Compiler core | `main` (#4), rates in #9 | Passes. A parameter value is in its declared unit, also for an expression with no unit. |
| 1.2 Runtime and first solvers | `main` (#2) | Passes. |
| 1.3 First blocks | `main` (#3), variadic Sum and PID rate in #9 | Passes. |
| 1.4 Service, Python, CLI | `main` (#5), compile and simulate in #10 | Passes in #10. The PID example is built, saved, compiled and simulated from Python. |
| 1.5 Conformance harness | `main` (#7) | Passes. 9 Simulink R2026a models. 12 cases pass, and 6 are expected to fail (WP2.1, WP2.3). `scripts/check.sh` runs it. |
| 1.6 MCP server | #11 | Passes. A scripted agent builds, runs and fixes the PID example through `edit`, and through file edits and `check`. |
| 1.7 Read-only viewer | #12 | Passes. All 11 example and conformance models render, 10 of them without layout entries. |
| 1.8 Expression engine | `main` (#1) | Passes. A number without a unit is unspecified. |
| **G1** | #10, #11 | **Passes on the branches.** `pid_plant` matches its Simulink ode4 reference within 1e-12 from Python, from the CLI (`simucraft simulate`) and through MCP. |

## Owner decisions on 2026-11-08 (done)

- Editor stack: Tauri, React and React Flow. The service is a sidecar on stdio. ADR 0003 (#8).
- A number without `[unit]` is unspecified (#1, merged).
- A parameter value is in its declared unit, and the loader converts it to SI (#1 and #4, merged).
- 9 reference models are sufficient for WP1.5 (`docs/work-packages.md` in #8).

## Known limits

- **Contract.** #9 closes the two gaps from the last handoff: variable input count and sample time in `build`. `core.PID` has no `ts` parameter now. Use the block's `ts` field.
- **Engine.** No zero crossings, no algebraic loops, no rate transitions (WP2.1). Mixed discrete rates take the faster one. `Bdf` starts a new diffsol problem at each major step.
- **Blocks.** No Sine, Zero-Order Hold, Rate Transition, Unit Delay or integrator limits (WP2.3). The three conformance models that need them are expected to fail. Their `.scmodel` files use Simulink names (`core.Sine` and so on), so WP2.3 must use those names or change the files.
- **Service.** Simulations run in the service process, not in workers. Runs stay in memory. `simulate` takes its solver from the model's `[simulation]` table, and `overrides` gives `A003` (WP2.4). No journal and no file watching (WP2.9).
- **MCP.** Each tool reads the project from disk again, so MCP has no undo. `diff` compares one file with a Git ref or another file. The full `diff` and `merge` commands are WP2.5.
- **Viewer.** It shows only the ports that are connected. It does not draw wire routes. The Tauri shell is not in `scripts/check.sh` (WP4.2).
- **Spec.** Section 7 still says "local socket". ADR 0003 records stdio for the editor.

## Next: Phase 2

Gate G1 is reached when #8 to #12 are merged. Then start wave 2a (`docs/work-packages.md`, Section 4):

1. **WP2.7** acausal compiler. It is on the longest dependency chain, so start it first.
2. **WP2.1** engine depth: zero crossings, algebraic loops, multi-rate. It turns 4 of the 6 expected fails into passes.
3. **WP2.4** experiment engine. It adds `overrides` and a results store.

WP2.3 (full block library) turns the remaining expected fails into passes. Remove each "expected to fail" mark in `conformance/src/main.rs` when its case passes; the harness fails until you do.

## Decisions the owner must make

None are blocking now. Each pull request lists the readings it chose under "Decisions".

## Notes on the local machine

- `scripts/check.sh` sets `VIRTUAL_ENV` to `.venv` and ignores conda (#8). Until #8 is merged, run `env -u CONDA_PREFIX VIRTUAL_ENV=$PWD/.venv scripts/check.sh`.
- An old editable `simucraft` is still in the conda base environment. To remove it, run `~/miniconda3/bin/python -m pip uninstall simucraft`.
- Node.js 24 and npm 11 are installed. The editor's packages were installed with `npm --before=2026-10-25`.
- To see the viewer in a browser: `cargo build -p simucraft-service`, then `npm --prefix editor run dev`, then open `http://localhost:5173/?project=<folder>`. See `editor/README.md`.

## Plugins

`.claude/settings.json` enables the plugins. In this session, `i-have-adhd` and the `handoff` skill of `mattpocock-skills` were not available, so this file follows the structure of the previous handoff.
