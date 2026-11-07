# Handoff: Phase 1, first five work packages in review

Date: 2026-11-07. The project passed gate G0. It did not pass gate G1 yet.

## Next action for the owner

1. Review and merge the pull requests in this order. Each later one needs the earlier ones.
   1. [#1 WP1.8 Expression engine](https://github.com/djangovanderplas/SimuCraft/pull/1)
   2. [#3 WP1.3 First blocks](https://github.com/djangovanderplas/SimuCraft/pull/3)
   3. [#4 WP1.1 Compiler core](https://github.com/djangovanderplas/SimuCraft/pull/4). This branch contains #1 and #3. Merge them first, and the diff shows only the compiler.
   4. [#2 WP1.2 Runtime and first solvers](https://github.com/djangovanderplas/SimuCraft/pull/2)
   5. [#5 WP1.4 Model API service, Python package and CLI](https://github.com/djangovanderplas/SimuCraft/pull/5)
2. Read the "Decisions" section of each pull request. Each one lists the readings of the spec that it chose. Reject the ones you do not agree with.
3. After the merges, run `scripts/check.sh` on `main`. All five branches add an entry to the root `Cargo.toml`, so expect small conflicts in `Cargo.toml` and `Cargo.lock`.

## State

| WP | Pull request | Crate | Done-when result |
| --- | --- | --- | --- |
| 1.8 Expression engine | #1 | `simucraft-expr` | Passes. All 14 expressions in `examples/` parse, unit-check and evaluate. Symbolic derivatives match central differences. |
| 1.2 Runtime and first solvers | #2 | `simucraft-runtime` | Passes. Dormand-Prince at rtol 1e-6 matches the mass-spring-damper within 1e-5. Euler, RK4 and diffsol BDF also pass. |
| 1.3 First blocks | #3 | `simucraft-blocks` | Passes. 11 block types, each with contract tests on `fake::run_euler`. |
| 1.1 Compiler core | #4 | `simucraft-compiler` | Passes. The PID example and the signal-flow part of `examples/motor_demo` compile. Each error names a block and a port, or a block and a parameter. |
| 1.4 Service, Python, CLI | #5 | `simucraft-service`, `python/` | Partly done. Python builds and saves the PID example. `compile` and `simulate` wait for G1. |

`scripts/check.sh` passes on each branch.

No pull request changes `crates/simucraft-contracts`.

## Known limits

- **WP1.3 without the Python Function block.** That block needs worker processes (WP2.9).
- **Contract gaps, no change yet.** `BlockSpec` cannot declare a variable number of ports. The compiler takes the ports of `core.Sum` from its `signs`. `BlockType::build` gets no sample time. The compiler passes the `ts` field of a block as the parameter `ts`. Decide at G1 if these need a contract change.
- **Physical networks.** The compiler skips nets and `core.Connector` blocks and gives warning `C009`. The motor demo test uses stub types for the `elec.*` and `rot.*` blocks.
- **Units.** Only parameter expressions are unit-checked. Units on ports come with WP2.2.
- **Runtime.** No zero crossings and no algebraic loops (WP2.1). `Bdf` starts a new diffsol problem at each major step.
- **Service.** No journal, no file watching, and `project.save` does not check for newer files on disk (WP2.9). The transport is JSON-RPC on stdio (`simucraft serve`), not a socket.

## Next: gate G1

1. Merge #1 to #5.
2. Add `compile` and `simulate` to the service. Use `simucraft_compiler::compile` with `simucraft_blocks::library()`. Then run the result with `simucraft_runtime::Simulation`. Add `Model.compile()` and `Model.simulate()` to the Python package.
3. Run the PID example end to end from Python: build, save, compile, simulate. This is the full WP1.4 done-when test.
4. Then start WP1.6 (MCP server), which needs WP1.4.

WP1.5 still needs your Simulink reference runs. WP1.7 still needs the editor stack decision.

## Decisions the owner must make

1. **Editor stack before WP1.7.** Choose Tauri or Electron.
2. **Reference runs for WP1.5.** Export Simulink results with the student license.
3. **Bare numbers in expressions.** WP1.8 makes a number without `[unit]` dimensionless. Thus `x + 1` is a unit error when `x` has a unit. The other reading is "unspecified".

## Notes on the local machine

- The repository was not on this machine. The session cloned it to `~/Documents/SimuCraft/repo`.
- `uv` was not installed. The session installed it with Homebrew, because `scripts/check.sh` needs it.
- With a conda environment active, `maturin develop` installs into conda, not into `.venv`. Run the check with `env -u CONDA_PREFIX VIRTUAL_ENV=$PWD/.venv scripts/check.sh`. A small fix in `scripts/check.sh` can export `VIRTUAL_ENV` itself.
- Before the session found this, one run installed an editable `simucraft` into the conda base environment. To remove it, run `~/miniconda3/bin/python -m pip uninstall simucraft`.

## Plugins

`.claude/settings.json` enables the plugins. In this session, the `handoff` skill of `mattpocock-skills` was not available, so this file follows the structure of the previous handoff.
