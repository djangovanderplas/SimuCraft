# Handoff: Phase 0 done, Phase 1 next

Date: 2026-11-07. Gate G0 (contracts frozen) is passed.

## State

| WP | Result | Where |
| --- | --- | --- |
| 0.1 Repository and tooling | Cargo workspace, Apache-2.0, Python package (maturin, abi3, Python >= 3.11), `scripts/check.sh` | root, `python/`, `scripts/` |
| 0.2 File formats | Schemas for the six file kinds, `check()` with codes F001-F010, format-preserving edits, stable IDs | `crates/simucraft-format`, `examples/motor_demo` |
| 0.3 Contracts | Diagnostics, block contract, compiled model (IR), Model API (42 methods), fakes, contract tests | `crates/simucraft-contracts` |
| 0.3 Execution decision | Interpreter in the MVP; Jacobians symbolic, block-declared or by finite differences; diffsol as solver backend | `docs/adr/0001-*.md`, `spikes/solver-backends` |

`scripts/check.sh` passes: 20 Rust tests, 4 Python tests, clippy clean.

## Known limits of Phase 0

- F3 (automatic migration): there is only format version 0.1, so there is
  nothing to migrate. `check()` rejects other versions with F003. Add the
  migration chain when version 0.2 of a format exists.
- `check()` checks one file at a time. Checks across files (a `ref` to a
  missing subsystem, layout keys that are not in the model, block types that
  are not in a library) belong to the compiler (WP1.1).
- TOML 1.1-only escapes (`\e`, `\xHH`) are not rejected by the Rust check.
  The Python test reads every example with `tomllib` (TOML 1.0) to catch them.
- The research findings (`docs/research.md`) say "SUNDIALS". ADR 0001
  replaces that: the backend is diffsol.

## Next: Phase 1

These work packages need only Phase 0 and can run in parallel, one branch
each. Recommended order (longest chain first):

1. **WP1.8 Expression engine** (lane D, `crates/simucraft-expr`). Spec
   Section 9. Python-like grammar, units in brackets (`24.0[V]`), UCUM unit
   strings, evaluator, compact bytecode, symbolic derivatives. Done when:
   every expression in `examples/` parses, unit-checks and evaluates; the
   symbolic derivatives match finite differences.
2. **WP1.2 Runtime and first solvers** (lane A, `crates/simucraft-runtime`).
   Interpreter over `ir::CompiledModel`; fixed-step Euler and RK4;
   variable-step Dormand-Prince; diffsol behind a `Solver` trait (ADR 0001).
   Done when: `crates/simucraft-contracts/tests/fixtures/mass_spring_damper.json`
   matches the analytic solution with Dormand-Prince at rtol 1e-6.
3. **WP1.3 First blocks** (lane B, `crates/simucraft-blocks`). Constant,
   Step, Gain, Sum, Integrator, Transfer Fcn, PID, Saturation, Scope, and the
   port blocks (`core.Inport`, `core.Outport`). The Python Function block needs
   worker processes (WP2.9), so leave it until then. Done when: each block
   has contract tests on `fake::run_euler` or the real runtime.
4. **WP1.1 Compiler core** (lane A, `crates/simucraft-compiler`). Model files
   to `ir::CompiledModel`: parameter evaluation (with WP1.8), types, widths,
   rates, flattening of subsystems, sorting, algebraic-loop detection. Done
   when: `examples/motor_demo` signal-flow part compiles, and every error
   names a block and a port.
5. **WP1.4 Model API service, Python, CLI** (lane C). The service implements
   the `api::Command` set over JSON-RPC, using `simucraft-format` for edits.
   Start with project, block, wiring, parameter, query and session commands.
   `compile` and `simulate` join at gate G1 when 1.1 and 1.2 are merged.

Later in Phase 1: WP1.5 (needs the owner's Simulink runs), WP1.6 (MCP, after
1.4), WP1.7 (viewer, after the editor stack decision below).

## Decisions the owner must make

1. **Editor stack before WP1.7.** The research recommends a TypeScript canvas
   in a desktop shell. Choose Tauri (smaller, Rust backend) or Electron.
2. **Reference runs for WP1.5.** The owner exports Simulink results with the
   student license.

## Plugins

`.claude/settings.json` enables the five plugins and registers their
marketplaces. The marketplace repositories in that file are a best guess.
Run `claude plugin marketplace list` locally and correct any source that
differs. `CLAUDE.md` holds the same rules, so the work is correct even if a
plugin does not load.
