# SimuCraft: decisions and requirements log

Last updated: 2026-11-07 (Phase 0 done)

## Goal
SimuCraft: an open-source alternative to Simulink with a Python API, plugin support and an MCP server for AI agents. MVP = base Simulink functionality plus a small acausal library, experiments, text files with diff/merge; no toolboxes, plugin-ready.

## Decisions
- Name: SimuCraft.
- Modeling paradigm: causal and acausal (Simscape-like) from the start.
- Use case: desktop simulation first; architecture ready for C code generation, HIL and ROS 2 later.
- Interop: .slx import and FMU import in v1; ROS 2 co-simulation in v1; code generation later.
- Scale: the model sizes Simulink handles.
- Platform: desktop first.
- Team: built by the owner with AI assistance; open-source release after the MVP. Recommended license: Apache-2.0.
- Student Simulink license is fine for reference runs (owner's call).
- No contact with PathSim maintainer and no user interviews for now; feedback will come after open-sourcing.
- Structured H-infinity tuning: far down the roadmap, as a plugin, not part of the MVP (freedom-to-operate review before building it).

## External review applied (2026-11-07)
An external review of the spec flipped six decisions and filled four gaps. All are now in the MVP spec (rev 21) and work packages.
- Expression language: flipped from "safe subset of Python" to one small Python-like grammar parsed and evaluated in Rust, unit-aware, shared by parameters, mask-derived parameters, acausal equations, metrics and requirements (spec Section 9, new requirement E8, new WP1.8). Reasons: no Python needed to compile, symbolic work for the acausal compiler, C codegen later, no "safe Python" sandbox.
- TOML: stay on TOML 1.0 (Python tomllib reads 1.1 only from 3.15); no inline tables, nested values as subtables with one key per line so plain Git merges per line; format-preserving writer (toml_edit) so comments survive; simucraft fmt for canonical rewrites; new tables inserted in ID order.
- IDs: 12 lowercase Crockford base32 characters (60 bits) with prefixes m_, b_, n_, a_; about 1 in 20 billion collision chance at 10,000 blocks. Merge still checks.
- Addressing: model inports, outports and physical connectors are blocks (core.Inport, core.Outport, core.Connector); every port is <block>.<port>. No separate p_ IDs.
- RNG: Philox streams keyed by (seed, stable key = owning block or model ID + parameter name), not by path; vary entries can pin a key with stream = "..."; manifest records keys. LHS is reproducible only for the same seed and run count.
- MCP: six task-level tools (check, edit with dry run, simulate, signal_summary, diff, library_docs) plus model resources, instead of ~40 tools mirroring every command. Full RPC stays for the editor and Python. Agents may also edit files and call check.
- Masks and trust: masks are declarative (prompts, derived parameters as expressions, icon); Python hooks are opt-in. Projects open restricted until the user trusts the folder (VS Code model); trust lives in user settings; MCP cannot grant trust (new requirement S1).
- Execution model (gap): MVP interprets a flat compiled schedule; expressions compile to bytecode; Cranelift JIT after the MVP unless WP0.3 benchmarks say otherwise. Jacobians: symbolic (expression engine), block-declared, or sparse finite differences. Solver backend: diffsol evaluated as default in WP0.3, SUNDIALS as reference (check license, events, maturity).
- Process model (gap): model service (Rust, holds state, runs no user code) + separate simulation workers (Python blocks, trusted hooks, native plugin blocks) + clients. Worker crash fails one run; service restarts from files plus journal (requirement E7 rewritten).
- Files vs service (gap): files are the source of truth; file watching; clean reload when no unsaved edits; three-way merge with unsaved edits, conflicts kept as a pending change set; save never overwrites a newer file; batched reloads for git operations (new requirement F4).
- Units policy (gap): declared, dimensionless ("1") or unspecified; unspecified never errors and units are inferred where possible; declared mismatches are errors; units.strict turns unspecified-to-declared into warnings; imported .slx models arrive unspecified.

## MVP spec (2026-11-06, revised 2026-11-07)
See the SimuCraft MVP Specification doc: https://claude.ai/artifact/6o9PThiQNEHFaxDgF5K2up
- 33 requirements (M1-M10 modeling, E1-E8 engine, X1-X4 experiments, F1-F4 files, P1 Python, U1-U2 editor, A1 MCP, G1 plugins, D1 design metadata, S1 security).
- File formats (v0.1), all TOML 1.0 with custom extensions: simucraft.toml (manifest), simucraft.lock, .scmodel (model), .sclayout (layout), .scexp (experiment), .scdesign (design study).
- Stable IDs as table keys; names unique per level; paths use "/" between instances and "." for members (guidance/pid.Kp).
- Signal connections stored at the destination input (one line per input); acausal connections as nets.
- Experiments: grid/monte_carlo/lhs/sobol sampling; Philox counter-based RNG keyed by (seed, stable key), counter (run, sample); results as parquet with manifest.
- Design metadata: tunable/uncertain/analysis_point/trace fields in models; operating points, tunable forms, loops and requirements in .scdesign; requirement kinds step_tracking, margins, sensitivity, max_gain/min_gain, poles, variance, assert, each with hard flag.
- Model API: JSON-RPC over a local socket to the model service; transactional, undoable, journaled commands that return diffs and support dry runs; events include files.reloaded and sync.conflict; project.trust is editor/CLI only.
- New sections: 9 Expressions and units, 10 Runtime architecture (processes, execution, file sync, trust), 11 Confirmed decisions.
- All open points confirmed by the owner on 2026-11-07: TOML 1.0 without inline tables; stable IDs as keys; 12-character IDs; own Rust expression language; UCUM unit strings; unit literals as 24.0[V]; unspecified units silent by default (units.strict for warnings); interpreted schedule in the MVP, Cranelift JIT later; diffsol as solver backend (ADR 0001); separate .sclayout files.

## Work plan (2026-11-07)
See the SimuCraft Work Packages doc: https://claude.ai/artifact/W3AHWRSjNWGCyx5z2zarik
- 27 work packages, 6 lanes (A engine, B blocks/conformance, C API/experiments, D files/expressions/collaboration, E physical modeling, F editor), 4 gates (G0 contracts frozen, G1 end-to-end slice, G2 core complete, G3 release).
- Phase 0: WP0.1 repo/tooling, WP0.2 file schemas + format-preserving reader/writer, WP0.3 interface contracts plus the execution decision (interpreter/JIT, Jacobian sources, diffsol vs SUNDIALS spike), now size L. Nothing parallel before G0.
- Changes from the review: new WP1.8 expression engine (lane D, L); WP1.6 is the six-tool MCP server; WP2.2 adds declarative masks, unit inference and project trust; WP2.9 becomes process model and file sync (L, depends on 2.5); WP2.4 and WP2.7 depend on 1.8.
- Parallel rules: contracts first, lane-owned directories, explicit contract-change PRs, contract tests + conformance suite, one branch/worktree per WP.
- Recommended max 3 concurrent WPs (owner is single reviewer). Longest chain: 0.1, 0.3, then 1.1 and 1.8, then 2.7 (acausal compiler) and 2.8; start 2.7 right after G1. Editor viewer (1.7) can start in Phase 1.

## Phase 0 (2026-11-07)
Repository: https://github.com/djangovanderplas/SimuCraft (private until the first release).
- WP0.1: Cargo workspace (Rust 2024), Apache-2.0, Python package via maturin (abi3, Python >= 3.11), scripts/check.sh as the only check. No CI while the repo is private (moved to WP4.2). The owner reserves the PyPI name.
- WP0.2: crate simucraft-format: schemas for the six file kinds, check() with diagnostic codes F001-F010, format-preserving File edits (toml_edit), IDs. examples/motor_demo has one file of each kind; open + save is byte-identical; Python tomllib reads all of them.
- WP0.3: crate simucraft-contracts: diagnostics, block contract, compiled-model IR (JSON), Model API (42 JSON-RPC methods), fakes (4 blocks, Euler runtime, in-memory API). ADR 0001: interpreter costs ~9 ns per block call; diffsol beat CVODE on stiff Robertson (115 vs 180 us, both in tolerance) and located events to 5e-14; diffsol is MIT. ADR 0002: contract shapes (f64 slots, contiguous ports, DAE as one block, fixes as API commands).
- Next: Phase 1 in Claude Code; see docs/handoff.md in the repo.

## Research docs
- Compiled findings: claude/SimuCraft_Research_Findings.md
- Market research: https://claude.ai/artifact/8u5LheUcAXuWNSMtgDh5B3
- Pain points and opportunities: https://claude.ai/artifact/6ksKBgTt39TYwTB3XK3e56
