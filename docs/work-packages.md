> Exported from the living doc on claude.ai on 2026-11-07. This copy is the reference for code work. If a work package must change it, change it here and say so in the pull request; the owner keeps the claude.ai doc in step.

# SimuCraft Work Packages

Nov 7, 2026 · @Django van der Plas

## Summary

The MVP splits into 27 work packages across six lanes and four gates. After the first gate (contracts frozen), up to six lanes could run at once; the practical limit is about three, because you are the single reviewer and integrator.

- **Sequential start:** Phase 0 (three packages) defines the file schemas and the interfaces between components. Nothing parallel is safe until these are frozen.
- **Six lanes:** engine, blocks and conformance, API and experiments, files, expressions and collaboration, physical modeling, editor. Each lane owns its own directories, so parallel branches rarely conflict.
- **Four gates:** G0 contracts frozen, G1 end-to-end slice, G2 core complete, G3 MVP release. Lanes sync at gates, where everything is integrated and the conformance suite must pass.
- **Surprise from the dependency analysis:** the editor does not have to wait. A read-only viewer only needs the file formats, so it can start right after G0 and grow into the visual diff tool.
- **Bottleneck:** with AI writing most of the code, the limiting factor is your review and integration time, not coding speed. Small work packages with clear done-when tests keep reviews short.

Sizes are relative (S, M, L, XL) rather than dates, since the pace of AI-assisted development is not yet known.

## 1. Rules for parallel work

Parallel lanes only pay off if they rarely block or break each other. Five rules make that true:

1. **Contracts first.** WP0.3 freezes four interfaces: the block contract, the compiled-model format (the intermediate representation between compiler and runtime), the Model API command schema, and the diagnostics format. It also settles how the compiled model runs: interpreter or JIT, where Jacobians come from, and which solver backend. Each lane builds against these and uses fakes for the others: the runtime can run hand-written compiled models before the compiler exists.
2. **Each lane owns its directories.** One repository, with separate folders per lane (for example `core/compiler`, `core/runtime`, `blocks/`, `python/`, `mcp/`, `editor/`). A lane edits only its own folders, plus shared contracts through a change request.
3. **Contract changes are explicit.** A change to a frozen interface is its own small pull request, reviewed before any lane builds on it. This prevents silent drift between lanes.
4. **Tests guard the seams.** Each contract has tests that both sides run, and the Simulink conformance suite (WP1.5) is the shared definition of "correct" for every lane.
5. **One branch per work package.** Each runs in its own Git worktree and AI session, merges into main when its done-when test passes, and is small enough to review in one sitting.

At each gate, all lanes integrate on main, run the full test and conformance suites, and fix what broke before new packages start.

## 2. Work packages

Lanes: **A** engine, **B** blocks and conformance, **C** API and experiments, **D** files, expressions and collaboration, **E** physical modeling, **F** editor. Spec IDs refer to the MVP Specification.

| WP | Work package | Lane | Size | Depends on | Delivers | Done when |
| --- | --- | --- | --- | --- | --- | --- |
| 0.1 | Repository and tooling: monorepo, Apache-2.0, one local check script (CI waits until the repository goes public); the owner reserves the PyPI name | All | S | — | Setup | One local command builds and tests the Rust core and the Python wheel |
| 0.2 | File schemas, reader and format-preserving writer (toml\_edit) for all six file types: TOML 1.0, no inline tables, 12-character IDs | D | M | 0.1 | F1, F3 | Every example file in the spec validates; open and save is byte-identical; a hand-written comment survives an edit elsewhere |
| 0.3 | Interface contracts: block contract, compiled-model format, Model API command schema, diagnostics format; fakes and contract tests. Execution decision: interpreter or JIT, Jacobian sources, solver backend (a diffsol versus SUNDIALS spike) | All | L | 0.1 | Enables all lanes | Each contract is documented, has a fake implementation and passing contract tests; the execution decision is written down and backed by benchmark runs |
| **G0** | **Gate: contracts frozen** |  |  | 0.1–0.3 |  |  |
| 1.1 | Compiler core: parameter evaluation, type, size and rate propagation, flattening, sorting, loop detection | A | L | 0.2, 0.3 | E1 | The PID example compiles; every error names a block and port |
| 1.2 | Runtime and first solvers: schedule interpreter, simulation loop, Dormand–Prince, Euler, RK4, logging stream, on the solver backend chosen in 0.3 | A | L | 0.3 | E2 (part) | A hand-written compiled mass-spring-damper matches the analytic solution |
| 1.3 | First 10 blocks: Constant, Step, Gain, Sum, Integrator, Transfer Fcn, PID, Saturation, Scope, Python Function | B | M | 0.3 | M9, M10 (part) | Each block passes its contract tests |
| 1.4 | Model API service, Python package and CLI | C | L | 0.2, 0.3 | P1 | The PID example is built, saved, compiled and simulated from Python |
| 1.5 | Conformance harness: MATLAB export script, reference-run store, trace comparison with tolerances, first 10 reference models | B | M | 0.2 | Verification for all | A comparison report runs in CI against stored Simulink results |
| 1.6 | MCP server with six task-level tools (check, edit with dry run, simulate, signal summary, diff, library docs) and model resources | C | S | 0.3, then 1.4 | A1 | An agent builds, runs and fixes the PID example, through edit and by editing files then calling check |
| 1.7 | Read-only diagram viewer with subsystem navigation and auto-placement | F | M | 0.2 | U1 (part) | All example models render, including ones without layout entries |
| 1.8 | Expression engine: parser, unit checking and inference, evaluator, bytecode, symbolic derivatives | D | L | 0.3 | E8, M6 (part) | Every expression in the spec's example files parses, unit-checks and evaluates without Python; symbolic derivatives match finite differences |
| **G1** | **Gate: end-to-end slice** |  |  | 1.1–1.8 |  | PID with plant runs from Python, CLI and MCP and matches Simulink |
| 2.1 | Engine depth: multi-rate, zero crossings, algebraic loops | A | L | 1.1, 1.2 | M7, E3, E4 | Bouncing ball, saturating integrator and a multi-rate model match references |
| 2.2 | Structure and types: subsystems, declarative masks, libraries, buses, units with inference; project trust | A | L | 1.1, 1.8 | M2–M6, S1 | A masked library subsystem with bus signals and units compiles and runs; a model without units compiles clean; an untrusted project runs no mask hook |
| 2.3 | Block library to the full MVP list | B | L | 1.3, 1.5 | M10 | Every MVP block passes conformance |
| 2.4 | Experiment engine: .scexp, samplers, Philox random streams, local executor, results store | C | L | 1.2, 1.4, 1.8 | X1–X4 | The spec's experiment runs; run 412 replays identically on 1 or 16 workers and after the varied block is renamed |
| 2.5 | Semantic diff, three-way merge, Git merge driver | D | M | 0.2 | F2 | Branches editing different blocks merge with no conflict |
| 2.6 | Design metadata validation | D | S | 0.2 | D1 | A .scdesign with a typo gives a clear error |
| 2.7 | Acausal compiler: alias elimination, Pantelides with dummy derivatives, tearing, initialization, symbolic Jacobians, DAE solver | E | XL | 0.3, 1.1, 1.8 | E5, M8 (part) | A Cartesian pendulum (index 3) simulates |
| 2.8 | Acausal starter library: electrical, translational, rotational, thermal | E | M | 2.7 | M8, M10 | The DC motor example matches its Simscape reference |
| 2.9 | Process model and file sync: model service and worker processes, incremental compilation and cache, command journal, file watching with three-way reload | A | L | 2.2, 1.4, 2.5 | E6, E7, F4 | Performance targets met; killing a worker or the service loses no edits; a git checkout under an open editor keeps unsaved edits |
| 2.10 | Plugin SDK: entry points, native block ABI, solver plugins | B | M | 1.3, 1.4 | G1 | A third-party block library installs from pip and runs |
| 3.1 | Editing: library browser, drag and drop, auto-routed wiring, mask dialogs, undo | F | L | 1.4, 1.7 | U1 | A new user rebuilds the PID example in the editor |
| 3.2 | Scopes, signal viewer, run comparison, diagnostics panel | F | M | 1.4, 2.4 | U2 | A diagnostic click selects the offending block |
| 3.3 | Visual model diff | F | M | 1.7, 2.5 | U2 | Two commits of a model show highlighted differences |
| **G2** | **Gate: core complete** |  |  | 2.1–3.3 |  | All 33 MVP requirements pass their done-when tests |
| 4.1 | Documentation, tutorials, example models | All | M | G2 | Release | A newcomer follows the tutorial end to end |
| 4.2 | CI on Linux, macOS and Windows, packaging, installers, performance hardening | All | M | G2 | Release | One-step install on all three systems |
| 4.3 | Public release | All | S | 4.1, 4.2 | Release | Repository public, packages published |
| **G3** | **Gate: MVP released** |  |  | 4.1–4.3 |  |  |

## 3. Lanes and gates

The physical-modeling chain decides when the MVP is done. The files lane starts in Phase 0, and the editor starts in Phase 1, far earlier than a strict phase order would allow.

| Lane | Phase 0 → G0 | Phase 1 → G1 | Phases 2 and 3 → G2 | Phase 4 → G3 |
| --- | --- | --- | --- | --- |
| All lanes | **0.1**, **0.3** | | | 4.1, 4.2, 4.3 |
| A Engine | | **1.1**, 1.2 | 2.1, 2.2, 2.9 | |
| B Blocks, conformance | | 1.3, 1.5 | 2.3, 2.10 | |
| C API, experiments | | 1.4, 1.6 | 2.4 | |
| D Files, expressions | 0.2 | 1.8 | 2.5, 2.6 | |
| E Physical modeling | | | **2.7**, **2.8** | |
| F Editor | | 1.7 | 3.1, 3.2, 3.3 | |

Bold marks the longest dependency chain: 0.1 → 0.3 → 1.1 → 2.7 → 2.8. Gates: G0 contracts frozen, G1 end-to-end slice, G2 core complete, G3 MVP released.

Phase 0 runs on the "all lanes" row because it defines the contracts every lane builds on.

## 4. Parallel waves

Run at most three packages at once, start the longest chain first, and fill gaps with the small, independent packages. The longest chain is the physical-modeling path (0.3, then 1.1 and 1.8, then 2.7 and 2.8), so WP2.7 starts as soon as G1 passes.

| Wave | Runs in parallel | Why this grouping |
| --- | --- | --- |
| 0a | 0.1 | Everything depends on it |
| 0b | 0.2, 0.3 | Schemas and contracts are independent of each other |
| 1a | 1.1, 1.2, 1.4 | The three large core packages; the contracts let compiler, runtime and API proceed without waiting for each other |
| 1b | 1.8, 1.3, 1.5 | The expression engine first, since the compiler and the acausal compiler need it; 1.5 needs your Simulink runs, so it overlaps with your review time |
| 1c | 1.7, 1.6, then G1 integration | The viewer only needs the file formats; MCP is small and needs 1.4 |
| 2a | 2.7, 2.1, 2.4 | Start the longest package (2.7) first; engine depth and experiments in parallel |
| 2b | 2.7 continues, 2.2, 3.1 | Editing only needs the API and viewer, so the editor starts here |
| 2c | 2.3, 2.5, 2.8 | Block library, diff and merge, physical library once 2.7 lands |
| 2d | 2.9, 2.10, 3.2 | Performance, plugins and scopes need the earlier engine work |
| 2e | 2.6, 3.3, then G2 integration | Small packages that close out the core |
| 3 | 4.1, 4.2, then 4.3 | Documentation and packaging in parallel, then release |

If you want more parallelism: the files lane (D) and editor lane (F) have few dependencies and can take an extra package at almost any time. The engine lane (A) and physical-modeling lane (E) touch the riskiest code, so keep at most one package running in each.

Where your attention matters most: reviewing 0.3 (every lane builds on it, and it settles how models run), producing the reference runs for 1.5, and checking 2.7 against the Pantelides and Mattsson papers.
