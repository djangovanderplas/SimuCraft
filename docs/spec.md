> Exported from the living doc on claude.ai on 2026-11-07. This copy is the reference for code work. If a work package must change it, change it here and say so in the pull request; the owner keeps the claude.ai doc in step.

# SimuCraft MVP Specification

Nov 6, 2026 · @Django van der Plas

## 1. Scope

The SimuCraft MVP is a desktop tool that builds and simulates the models base Simulink can, plus a small physical-modeling library. Every model, experiment and design setup is a text file in a Git repository, and the editor, Python and AI agents all drive one Model API.

**In the MVP**

- Signal-flow block diagrams: continuous, discrete, multi-rate and hybrid, with subsystems, masks and libraries
- Acausal networks with a starter library (electrical, 1D translational, 1D rotational, thermal) mixed with signal-flow blocks
- Compiler, solvers, zero-crossing detection and algebraic loop handling
- Experiments: parameter sweeps and Monte Carlo runs, run in parallel on the local machine, reproducible run by run
- Text project files with stable IDs, plus semantic diff and three-way merge
- Python package, desktop editor and MCP server on the same Model API
- Plugin SDK for block libraries and solvers
- The design-metadata schema (analysis points, tunable and uncertain parameters), without the tuning tools
- One unit-aware expression language for parameters, metrics and requirements, evaluated without Python
- Project trust: a cloned project runs none of its own code until the user trusts it

**Not in the MVP:** code generation, HIL and real time, ROS 2, FMU import and export, .slx import, linearization and tuning tools, state machines, fixed-point types, live co-editing. These are in Section 3.

The file formats here are version 0.1. They will change before 1.0, which is why every file carries a format version from day one.

## 2. MVP requirements

Thirty-three requirements in ten areas. Each has a done-when test so the MVP has a clear finish line; performance numbers are first targets to refine once real models run.

| ID | Area | Requirement | Done when |
| --- | --- | --- | --- |
| M1 | Modeling | Signal-flow diagrams with blocks, wires, branches and model-level inports and outports | A closed-loop PID and plant model builds and runs from the editor and from Python |
| M2 | Modeling | Subsystems: virtual, atomic, enabled and triggered; subsystems can live in their own file | A subsystem file is referenced from two models and edits show up in both |
| M3 | Modeling | Masks: declarative parameter dialog, derived parameters as expressions, icon; an optional Python hook that runs only in trusted projects | A masked subsystem behaves as a custom block; its hook does not run until the project is trusted |
| M4 | Modeling | Libraries: blocks linked to a versioned library, updated when the library changes | Upgrading a library version updates every linked block |
| M5 | Modeling | Signal types: floats, integers, booleans, vectors, matrices and one structured (bus) type | Bus signals pass through subsystems without manual bus objects |
| M6 | Modeling | Units on ports and parameters: declared, dimensionless or unspecified (Section 9) | Connecting m/s to rad is a compile error; a model with no units at all compiles |
| M7 | Modeling | Sample times: continuous, discrete, inherited and multi-rate, with rate transitions | A model with 1 kHz and 100 Hz parts matches its Simulink reference run |
| M8 | Modeling | Acausal networks: connectors with across and through variables, equation-based components, converter blocks to and from signals | A DC motor with a signal-flow speed controller simulates correctly |
| M9 | Modeling | Python Function block, run only inside simulation workers | A block defined by a Python function runs in a model |
| M10 | Modeling | Base block library: sources, sinks, continuous, discrete, math, nonlinear, lookup tables, logic, routing, buses; acausal electrical, translational, rotational and thermal | Each block passes a conformance test against a Simulink reference |
| E1 | Engine | Compiler passes: evaluate parameters; propagate types, sizes, units and rates; flatten; sort; detect algebraic loops | Every compile error names the block and port it concerns |
| E2 | Engine | Solvers: variable-step (Dormand–Prince, BDF), fixed-step (Euler, RK4), a DAE solver for acausal models; Jacobians from symbolic derivatives, block-provided forms or finite differences | Stiff and non-stiff reference models match within tolerance |
| E3 | Engine | Zero-crossing detection and location | A bouncing ball and a saturating integrator match their reference runs |
| E4 | Engine | Algebraic loop solving with a clear diagnostic when it fails | A loop is solved, or highlighted with the blocks involved |
| E5 | Engine | Acausal compiler: alias elimination, index reduction (Pantelides with dummy derivatives), tearing, consistent initialization | A pendulum in Cartesian coordinates (index 3) simulates |
| E6 | Engine | Incremental compilation with a content-hashed cache | Targets: 10,000-block model compiles in under 10 s; after a one-block edit, recompile in under 1 s |
| E7 | Engine | Process model: a model service holds project state; simulations run in separate worker processes (Section 10) | Killing a worker fails only that run; killing the model service loses no edits |
| E8 | Engine | Expression engine: one unit-aware expression language for parameters, equations, metrics and requirements, parsed and evaluated in Rust, with symbolic derivatives (Section 9) | Compiling a model needs no Python; a unit error points at the exact place in the expression |
| X1 | Experiments | Experiments from .scexp files: overrides, grids, Monte Carlo, Latin hypercube, Sobol | The sample experiment in Section 6 runs as written |
| X2 | Experiments | Counter-based random streams keyed by seed and stable keys (vary entry, block ID); any run can be replayed alone | Run 4711 gives identical results on 1 or 16 workers, and after the varied parameter is renamed |
| X3 | Experiments | Local parallel execution; the model compiles once per experiment | 1,000 runs scale close to linearly with core count |
| X4 | Experiments | Results store: per-run table plus optional traces, metrics and requirement checks | Results load into pandas with one call |
| F1 | Files | Text formats as in Sections 4 to 7, without inline tables, written by a format-preserving writer | Open and save without edits is byte-identical; a hand-written comment survives an edit elsewhere in the file |
| F2 | Files | Semantic diff and three-way merge, as CLI and Git merge driver | Two branches that edit different blocks merge with no conflict |
| F3 | Files | Format version in every file, with automatic migration | A 0.1 file opens in the next format version |
| F4 | Files | File sync: watch the project, reload external edits, merge them with unsaved changes, never overwrite a newer file | A Git checkout while the editor is open updates the diagram without losing unsaved edits |
| P1 | Python | pip-installable package to build, edit, simulate and run experiments; results as NumPy and pandas | The Python example in Section 8 runs |
| U1 | Editor | Canvas with library browser, drag and drop, auto-routed wires, subsystem navigation, mask dialogs, undo and redo | A new user rebuilds the PID example without docs |
| U2 | Editor | Scopes, signal viewer, run comparison, diagnostics panel with highlighting, visual model diff | A diagnostic click selects the offending block |
| A1 | MCP | Six task-level MCP tools: check, edit (transactional, with dry run), simulate, signal summary, diff, library docs | An agent builds, runs and fixes the PID example, both through edit and by editing files and calling check |
| G1 | Plugins | Block and solver plugin SDK; Python and native (Rust or C) blocks; distributed as pip packages | A third-party block library installs and appears in the browser |
| D1 | Design | Analysis points and tunable and uncertain parameters in models; .scdesign files parsed and validated | A .scdesign file with a typo gives a clear error |
| S1 | Security | Project trust: projects open restricted and run no project code (mask hooks, Python Function blocks, project-local plugins) until trusted; MCP cannot grant trust | Opening an untrusted project with a mask hook runs no Python |

## 3. Future improvements

Everything below already has a hook in the MVP's formats or API, so none of it needs a rewrite. Order within a phase is a suggestion.

| Phase | Improvement | Builds on |
| --- | --- | --- |
| v1 | FMU import (Model Exchange and Co-Simulation, FMI 2.0 and 3.0) | Block plugin contract (G1) |
| v1 | .slx importer plus a MATLAB-side exporter, with a block mapping table | Model format, unknown-block placeholders |
| v1 | Trim and linearization at analysis points; loop views (Bode, Nichols, margins, step) | Design metadata (D1), compiled model |
| v1 | PID tuner | Linearization |
| v1 | Cluster executors (Dask, Ray, Slurm) | Experiment engine (X1–X4) |
| v1 | CI action that posts a visual model diff on pull requests; pytest plugin | Diff engine (F2) |
| v1 | ROS 2 publish and subscribe blocks, paced real-time simulation, /clock support | Runtime pacing |
| v1 | Model reference and variants | Compile units in the model format |
| v1 | JIT compilation of right-hand-side and residual functions (Cranelift), unless WP0.3 pulls it into the MVP | Compiled model intermediate form |
| Later | C code generation from fixed-step models; FMU export | One compiled model for simulation and code |
| Later | ROS 2 node generation; real-time Linux targets and HIL with an open external-mode protocol | Code generation |
| Later | State machine blocks | Block contract |
| Later | Fixed-point and embedded integer types | Type system |
| Later | Live co-editing; review comments anchored to blocks | Model API command stream, stable IDs |
| Later | Batched or GPU runs, adaptive stopping, rare-event sampling | Experiment engine |
| Plugin | Aerospace library: 6-DOF, atmosphere, gravity, wind | Plugin SDK |
| Plugin | Multi-objective controller tuning, gain scheduling | Design metadata, linearization, experiments |
| Plugin, far | Structured H-infinity tuning, after a freedom-to-operate review | Tuning plugin |

## 4. Project layout and file formats

Every SimuCraft file is TOML 1.0 with its own extension: the extension tells people and tools what a file is, and any TOML library can read it. The format stays on TOML 1.0 and uses no inline tables. TOML 1.1 allows multi-line inline tables, but Python's standard library (tomllib) reads 1.1 only from Python 3.15, and an inline table puts several values on one line, which plain Git then diffs and merges as one change. Rust has mature TOML crates, including toml\_edit for edits that keep comments and layout.

### Why TOML

| Criterion | TOML | JSON | YAML |
| --- | --- | --- | --- |
| Comments | Yes | No | Yes |
| Line-based diffs and merges | Good: one key per line, tables keyed by ID | Fair: braces and commas cause noisy diffs | Good |
| Unambiguous types | Yes | Yes | No: unquoted values such as `no` or `1e3` can change type |
| Hand and AI editing | Easy | Verbose | Easy, but indentation errors are silent |
| Weak spot | Deep nesting is verbose |  |  |

The weak spot is avoided by design: models are kept flat (tables keyed by ID), nesting happens across files, one per subsystem, and nested values are written as subtables with one key per line.

### File types

| File | Extension | Holds |
| --- | --- | --- |
| Project manifest | `simucraft.toml` (fixed name) | Project name, top model, library dependencies, default simulation settings |
| Lock file | `simucraft.lock` | Exact library versions and hashes, written by the tool |
| Model | `.scmodel` | One model or subsystem: parameters, ports, blocks, connections, signals, design fields |
| Layout | `.sclayout` | Positions, wire routes and annotations for the `.scmodel` with the same name |
| Experiment | `.scexp` | A run, sweep or Monte Carlo campaign |
| Design study | `.scdesign` | Operating points, tunable-parameter forms, loops and requirements |
| Data | `.npz`, `.csv`, `.parquet` | Large arrays and lookup tables, referenced by path |

### Rules for all files

- The first key is format = "simucraft.\<kind>/\<version>", for example "simucraft.model/0.1".
- Stable IDs: models, blocks, nets and annotations get 12 random characters of lowercase Crockford base32 (60 bits) with a kind prefix: m\_, b\_, n\_, a\_ (for example b\_4a10f3k8m2qx). At 10,000 blocks the chance of any collision is about 1 in 20 billion, so IDs drawn independently on two branches do not clash; merge still checks and re-draws on a clash. IDs never change; names can.
- One address form: every port is \<block>.\<port>, by ID (b\_4a10f3k8m2qx.y) or by name (error.y). Model inports, outports and physical connectors are blocks too (core.Inport, core.Outport, core.Connector), so there is no separate port syntax.
- Names are unique within one model level. People and scripts address things by name through paths: / separates subsystem instances and . selects a member, so guidance/pid.Kp is parameter Kp of block pid inside subsystem guidance, and guidance/pid.y is its output port.
- No inline tables: nested values are subtables (\[blocks.\<id>.params\], \[blocks.\<id>.inputs\]) with one key per line, so plain Git and GitHub diff and merge them line by line. Arrays of numbers or port references stay on one line; the merge driver handles those.
- Format-preserving writer: saving edits the file in place (toml\_edit), rewriting only what changed, so hand-written comments and order survive. New content is written in canonical form: new tables go in ID order, which spreads additions from different branches through the file; keys in schema order; shortest round-trip numbers; LF line endings; no timestamps or user names. simucraft fmt rewrites a whole file canonically.
- References to other files are relative paths from the referring file.

### Project layout

```text
lander/
├── simucraft.toml
├── simucraft.lock
├── models/
│   ├── lander.scmodel
│   ├── lander.sclayout
│   └── subsystems/
│       ├── guidance.scmodel
│       └── guidance.sclayout
├── data/
│   └── aero_tables.npz
├── experiments/
│   └── dispersion.scexp
├── design/
│   └── pitch_loop.scdesign
├── .simucraft/        # compile cache, git-ignored
└── results/           # experiment outputs, git-ignored
```

### Project manifest

```toml
format = "simucraft.project/0.1"
name = "lander"
top = "models/lander.scmodel"

[libraries]                  # pip packages that provide blocks
simucraft-core = "0.1"
simucraft-aero = ">=0.3,<0.4"

[defaults.simulation]        # models and experiments can override
solver = "auto"
stop_time = 10.0
rel_tol = 1e-6
abs_tol = 1e-8
```

## 5. Model and layout files

A `.scmodel` holds logic only, as flat tables keyed by ID; its `.sclayout` twin holds the picture. The example is a PI speed controller (signal flow) driving a DC motor (acausal).

### Model file: `speed_control.scmodel`

```toml
format = "simucraft.model/0.1"
id = "m_4c1e9ah2kq7d"
name = "speed_control"
doc = "PI speed control of a DC motor"

[simulation]                    # optional; overrides project defaults
stop_time = 5.0

# Parameters: value (literal), expr (expression, Section 9) or file (data)
[parameters.Kp]
value = 0.8
unit = "V*s/rad"
tunable = true                  # design field, see Section 7

[parameters.Ki]
value = 4.0
unit = "V/rad"
tunable = true

[parameters.R]
value = 1.2
unit = "Ohm"

[parameters.R.uncertain]        # design field, see Section 7
dist = "normal"
std = 0.05

[parameters.friction_map]
file = "../data/motor.npz"
key = "friction"

# Model ports are blocks too, so every endpoint is <block>.<port>.
# New tables go in ID order, which spreads additions through the file.
[blocks.b_1c2d4r8vn3ex]
name = "speed_ref"
type = "core.Inport"

[blocks.b_1c2d4r8vn3ex.params]
dtype = "f64"
unit = "rad/s"

[blocks.b_2aa10gk5w2pm]
name = "speed"
type = "core.Outport"

[blocks.b_2aa10gk5w2pm.inputs]
u = "b_e5f20hp8k1wt.y"

# Signal-flow blocks: each input names its one source, one line per input
[blocks.b_4a10f3k8m2qx]
name = "error"
type = "core.Sum"

[blocks.b_4a10f3k8m2qx.params]
signs = "+-"

[blocks.b_4a10f3k8m2qx.inputs]
u1 = "b_1c2d4r8vn3ex.y"
u2 = "b_e5f20hp8k1wt.y"

[blocks.b_5c2d9tz6hyaw]
name = "pi"
type = "core.PID"
ts = 0.001                      # discrete at 1 kHz; omit to inherit

[blocks.b_5c2d9tz6hyaw.params]
P = "Kp"
I = "Ki"
D = 0.0

[blocks.b_5c2d9tz6hyaw.inputs]
u = "b_4a10f3k8m2qx.y"

# Acausal blocks: physical ports join through nets
[blocks.b_7d2c4nw7r1fs]
name = "supply"
type = "elec.ControlledVoltageSource"   # signal in, physical out

[blocks.b_7d2c4nw7r1fs.inputs]
v = "b_5c2d9tz6hyaw.y"

[blocks.b_9b18ejx4s9mk]
name = "motor"
type = "subsystem"
ref = "subsystems/dc_motor.scmodel"

[blocks.b_9b18ejx4s9mk.params]
R = "R"

[blocks.b_cf0f0q3zc6vn]
name = "ground"
type = "elec.Ground"

[blocks.b_e5f20hp8k1wt]
name = "speed_sensor"
type = "rot.SpeedSensor"               # physical in, signal out

[nets.n_2e60aa7v2d9b]
ports = ["b_7d2c4nw7r1fs.p", "b_9b18ejx4s9mk.p"]

[nets.n_6a2b3cm6t4qe]
ports = ["b_7d2c4nw7r1fs.n", "b_9b18ejx4s9mk.n", "b_cf0f0q3zc6vn.g"]

[nets.n_9c1d0x9g3krz]
ports = ["b_9b18ejx4s9mk.shaft", "b_e5f20hp8k1wt.flange"]

# Signal properties, keyed by the producing port
[signals."b_4a10f3k8m2qx.y"]
name = "speed_error"
log = true

[signals."b_5c2d9tz6hyaw.y"]
name = "voltage_cmd"
analysis_point = true           # design field, see Section 7
```

### Rules

- Connections live at the destination. A signal input has exactly one source, so each input is its own line in the block's inputs subtable: two branches that rewire different inputs change different lines and merge cleanly, even in plain Git. A branch is simply several inputs naming the same source.
- Model ports are blocks: core.Inport and core.Outport for signals, core.Connector (with a domain) for physical connections. In a subsystem file, these blocks become the ports of every subsystem block that references the file, named after the port block.
- Physical connections are nets: an unordered set of ports, sorted by the writer.
- Block type is library.BlockType; the library version is pinned in simucraft.lock, which is how library links work.
- Parameter values: a number, boolean or array is a literal. A string is an expression in the SimuCraft expression language (Section 9), evaluated against the model's parameters.
- Subsystems are type = "subsystem" with a ref to another .scmodel.
- Masks are a \[mask\] table in a subsystem file and are declarative: prompts for its parameters, derived parameters as expressions, and an icon. An optional Python init hook (hook = "masks/motor.py:init") is for cases expressions cannot cover; it runs only in a trusted project and only in a simulation worker (Section 10).

### Layout file: `speed_control.sclayout`

```toml
format = "simucraft.layout/0.1"
model = "speed_control.scmodel"

[blocks.b_4a10f3k8m2qx]
at = [140, 80]
size = [30, 30]
orient = "right"

[wires."b_5c2d9tz6hyaw.u"]       # a signal wire, keyed by its destination
points = [[200, 95], [240, 95]]

[nets.n_9c1d0x9g3krz]
points = [[410, 160], [460, 160]]

[annotations.a_19bc02f5n8jh]
text = "Speed loop, 1 kHz"
at = [120, 20]
```

A block without a layout entry is auto-placed by the editor, so scripts and agents can add blocks without inventing coordinates.

## 6. Experiment files

A `.scexp` describes everything about a batch of runs: the model, the inputs, what varies, what to record, what counts as a pass, and how to execute. A single simulation is an experiment with one run.

### Example: `motor_robustness.scexp`

```toml
format = "simucraft.experiment/0.1"
name = "motor_robustness"
model = "../models/speed_control.scmodel"
seed = 42
runs = 1000
sampling = "monte_carlo"        # grid | monte_carlo | lhs | sobol

[simulation]                    # overrides model settings
stop_time = 2.0

[inputs.speed_ref]              # drives a model inport by name
signal = "step"                 # constant | step | ramp | sine | table

[inputs.speed_ref.params]
time = 0.1
final = 100.0

[set]                           # fixed overrides, same in every run
"motor.friction_scale" = 1.1

[vary.R]                        # use the model's own 'uncertain' entry
from_model = true

[vary."motor.J_load"]
dist = "uniform"                # normal | uniform | lognormal | choice
low = 0.008
high = 0.016

[record]
signals = ["speed", "voltage_cmd"]
traces = "failures"             # all | none | failures | sample:50
decimate = 10

[metrics]                       # computed on the worker, one value per run
overshoot = "max(speed) / final(speed) - 1"
settling = "settling_time(speed, tol=0.02)"
peak_voltage = "max(abs(voltage_cmd))"

[requirements]                  # pass/fail per run
overshoot_ok = "overshoot < 0.10"
voltage_ok = "peak_voltage < 24.0[V]"
uses = ["../design/speed_loop.scdesign#no_saturation"]

[execution]
executor = "local"              # local in the MVP; dask, ray, slurm in v1
workers = "auto"
```

### Rules

- Paths address parameters and signals by name, as defined in Section 4.
- Sampling: grid takes the full product of each entry's values = \[...\]; monte\_carlo, lhs and sobol draw from each entry's dist. from\_model = true reuses the uncertain field declared on the parameter, so uncertainty is declared once.
- Randomness: every random stream uses a counter-based generator (Philox) keyed by (seed, stream key) with counter (run index, sample index). The stream key is the varied parameter's stable identity, not its path: the owning block's ID (or the model's ID) plus the parameter name. Renaming or moving the motor block leaves every draw unchanged. Renaming the parameter itself changes its key, so a vary entry can pin the old one with stream = "\<key>"; the results manifest records every entry's key. Run 412 draws the same values whatever the worker count, and can be replayed alone.
- Latin hypercube stratifies across all runs, so an lhs run is reproducible only with the same seed and the same runs. monte\_carlo and sobol (scrambled with the seed) give run 412 the same values whatever runs is.
- Metrics and requirements are expressions in the language of Section 9. Metrics add a documented set of signal functions: final, max, min, mean, rms, at(t), rise\_time, settling\_time, overshoot. Requirements are boolean expressions over metrics; uses imports named requirements from a .scdesign file, so control requirements double as simulation checks.
- Compile once: the model compiles once per experiment; only parameter values and initial states change between runs.

### Results layout

```text
results/motor_robustness/2026-11-06T14-03-11_a3f9c2/
├── manifest.toml      # experiment and model hashes, git commit, versions, seed
├── runs.parquet       # one row per run: varied values, metrics, pass/fail
└── traces/
    └── run_000412.parquet
```

`simucraft replay <results folder> --run 412` re-runs one case with full logging and scopes open. Results stay out of Git; the manifest makes any result traceable to the exact commit that produced it.

## 7. Design metadata

Design information lives in two places. Facts about the model itself (which parameters are tunable or uncertain, where loops can be opened) are fields in the `.scmodel`. A design study (operating points, controller forms, requirements) is a `.scdesign` file, so one model can have several studies. In the MVP these are parsed, validated and shown in the editor; the trim, linearization and tuning tools that use them arrive in v1 and in plugins.

### Fields in the model

| Where | Field | Meaning |
| --- | --- | --- |
| Parameter | tunable = true, optional min, max | A tuning tool may change it |
| Parameter | an uncertain subtable: dist = "normal" with std, dist = "uniform" with low and high, or rel for a relative spread | Used by experiments (from\_model = true) and later by robust analysis |
| Signal | analysis\_point = true | A loop can be opened, excited or measured here without editing the diagram |
| Block or signal | trace = \["REQ-FCS-012"\] | Links to external requirement IDs, for DO-178C-style traceability |

### Design study: `speed_loop.scdesign`

```toml
format = "simucraft.design/0.1"
name = "speed_loop"
model = "../models/speed_control.scmodel"

[operating_points.nominal]
method = "trim"                    # trim | snapshot | initial
free = ["speed_ref"]               # inputs or states the solver may adjust

[operating_points.nominal.outputs] # targets
speed = 100.0

[operating_points.load_range]      # a family of points, e.g. for scheduling
base = "nominal"

[operating_points.load_range.grid]
"motor.J_load" = [0.008, 0.012, 0.016]

[tunable.pi]                       # how a block is parameterized for tuning
block = "pi"
form = "pid"                       # scalar | pid | tf | ss | gain_surface

[tunable.pi.params]
P.min = 0.0
I.min = 0.0
D.fixed = 0.0

[loops.speed]
open_at = ["voltage_cmd"]          # analysis points

[requirements.settle_fast]
kind = "step_tracking"
from = "speed_ref"
to = "speed"
time_constant = 0.05
at = "load_range"                  # operating points it applies to
hard = false

[requirements.margins]
kind = "margins"
loop = "speed"
gain_db = 6.0
phase_deg = 45.0
hard = true

[requirements.no_saturation]
kind = "assert"                    # checked in simulations and experiments
expr = "max(abs(voltage_cmd)) < 24.0[V]"
```

### Requirement kinds in version 0.1

| Kind | Key fields | Checked on |
| --- | --- | --- |
| `step_tracking` | `from`, `to`, `time_constant` or `settling_time`, optional `overshoot` | Linear model or simulation |
| `margins` | `loop`, `gain_db`, `phase_deg` | Linear model |
| `sensitivity` | `loop`, `max_peak_db` | Linear model |
| `max_gain` / `min_gain` | `from`, `to`, `gain_db`, optional `band_hz` | Linear model |
| `poles` | `min_damping`, `min_decay`, `max_freq_hz` | Linear model |
| `variance` | `from`, `to`, `max` | Linear model |
| `assert` | `expr` over signals or metrics | Simulation |

Every requirement has `hard` (must hold versus best effort) and an optional `at` (operating points). This is the same split between hard and soft goals used in the multi-model tuning literature, so a future tuning plugin can read these files unchanged.

## 8. Model API commands

One set of commands serves the editor, the Python package and the MCP server, so anything one can do, all can. The model service (Rust) owns open projects and runs no user code; clients send commands as JSON-RPC 2.0 over a local socket, and simulations run in separate worker processes (Section 10). For scripts, the Python package can host a private model service in-process; its simulations still run in workers.

### Behavior shared by all edit commands

- **Transactional and undoable.** Each command is its own transaction unless grouped with `tx.begin` and `tx.commit`; undo and redo work on transactions.
- **Returns its diff.** Every edit returns the semantic diff it made, in the same form `diff` produces.
- **Dry run.** Any edit accepts `dry_run: true` and returns the diff without applying it, which lets agents and users preview changes.
- **Journaled.** Every applied command is appended to a journal, on disk, so an editor or model service crash loses no edits; a restarted service replays it.
- **Addressed by path or ID.** Arguments accept names (`pi.y`, `motor.J_load`) or IDs (`b_5c2d9tz6hyaw.y`); results always include IDs.
- **Structured diagnostics.** Errors and warnings come back as `{code, severity, path, id, message, fix}`.

### Commands

| Group | Command | Arguments | Returns |
| --- | --- | --- | --- |
| Project | `project.create`, `project.open` | path, name | project summary |
| Project | `project.save`, `project.status` |  | files written; unsaved changes and diagnostics |
| Project | project.trust | path; editor and CLI only, never MCP | trust state |
| Blocks | `block.add` | model, type, name, params, inputs | block ID |
| Blocks | `block.remove`, `block.rename`, `block.set_params` | block, name or params | diff |
| Wiring | `connect`, `disconnect` | source, destination | diff |
| Wiring | `net.connect`, `net.disconnect` | ports | net ID, diff |
| Interface | `port.add`, `port.remove` | model, direction, name, type, unit | block ID, diff |
| Parameters | `param.set`, `param.remove` | model, name, value or expr or file, unit, tunable, uncertain | diff |
| Signals | `signal.set` | port, name, log, analysis\_point, trace | diff |
| Structure | `subsystem.create`, `subsystem.expand` | blocks, name, file | block ID, diff |
| Layout | `layout.set`, `annotation.add` | block, position, size, orientation; text | diff |
| Session | `tx.begin`, `tx.commit`, `tx.rollback`, `undo`, `redo` | label | transaction ID |
| Query | `model.get`, `find` | path and depth; query | model tree; matches |
| Library | `library.list`, `library.describe` | filter; block type | block types; ports, parameters, docs |
| Run | `compile` | model | diagnostics |
| Run | `simulate`, `run.stop` | model, overrides, stop time, signals to record | run ID; streamed progress |
| Run | `signal.get` | run, signal path, mode (summary, decimated, full) | statistics or arrays |
| Experiments | `experiment.run`, `experiment.status` | .scexp file or inline spec | experiment ID; progress |
| Experiments | `experiment.results`, `experiment.replay` | experiment ID, what or run index | runs table and metrics; run ID |
| Versions | `diff`, `merge` | two refs (working tree, file, Git ref); base, ours, theirs | semantic diff; merged model and conflicts |
| Design | `design.validate` | .scdesign file | diagnostics |

Clients can also subscribe to events: `model.changed` (with the diff), files.reloaded, sync.conflict, `diagnostics.changed`, `run.progress` and `run.finished`. In v1, the Design group grows `trim`, `linearize` and `loop.analyze`.

### Python example

```python
import simucraft as sc

proj = sc.open("motor_demo/")
m = proj.model("models/speed_control.scmodel")

with proj.transaction("Add speed feedforward"):
    ff = m.add_block("core.Gain", name="ff", params={"k": 0.05})
    s = m.add_block("core.Sum", name="sum_ff", params={"signs": "++"})
    m.connect("speed_ref", ff.u)
    m.connect(ff.y, s.u1)
    m.connect("pi.y", s.u2)
    m.connect(s.y, "supply.v")       # replaces the previous source of supply.v

print(proj.diff())                    # semantic diff of unsaved changes
print(m.compile())                    # diagnostics, empty when clean

run = m.simulate(stop_time=2.0)
run.signal("speed").plot()

res = proj.experiment("experiments/motor_robustness.scexp").run()
print(res.runs.describe())
proj.save()
```

### MCP server

The MCP server gives agents six task-level tools instead of one tool per command, because long tool lists make agents choose worse. Project files are plain text, so an agent can also edit them directly and then call check; both routes end in the same diagnostics. The full command set stays available to the editor and to Python over JSON-RPC.

| Tool | What it does | Built on |
| --- | --- | --- |
| check | Compile a model or validate any project file; returns structured diagnostics | compile, design.validate |
| edit | Apply a list of edit commands as one transaction; dry\_run returns the diff without applying it | the edit commands, tx |
| simulate | Run a model with overrides, or an .scexp file; waits, or returns an ID to poll | simulate, experiment.run |
| signal\_summary | Statistics, key times and a decimated trace for named signals; the runs table for experiments | signal.get, experiment.results |
| diff | Semantic diff between the working tree, files or Git refs | diff |
| library\_docs | Search block types and read their ports, parameters, units and docs | library.list, library.describe |

Models are also served as MCP resources (the model tree by path), so agents can read them without a tool call. The MCP server cannot trust a project (Section 10), so an agent that opens a cloned repository cannot make it run project code.

## 9. Expressions and units

One expression language is used for parameter values, mask-derived parameters, acausal equations, experiment metrics and requirements. It is parsed and evaluated in Rust, so compiling a model never starts Python, the acausal compiler can simplify and differentiate expressions symbolically, and the same expressions can later be emitted as C. Anything the language cannot express goes in a Python Function block or a plugin, both of which run in simulation workers.

### Expression language

- Syntax reads like Python: numbers, names, + - \* / \*\* %, comparisons, and, or, not, a if c else b, function calls, indexing, and array literals such as \[1, 2\] and \[\[1, 0\], \[0, 1\]\].
- Names resolve by context: parameter expressions see the model's parameters and the constants pi and e; mask expressions add the mask's prompts; metrics see recorded signals; requirements see metrics and must be boolean.
- Functions come from a fixed library: abs, sqrt, exp, log, the trigonometric functions, atan2, min, max, clip, sign, interp and sum, plus the signal functions of Section 6 in metrics. Plugins can add pure functions, each with a unit rule and a derivative.
- There is no assignment, loop, attribute access, import or user-defined function, so evaluation always ends and cannot reach files, the network or the process.
- A number can carry a unit in brackets, such as 24.0\[V\] or 2\[ms\]. Units are checked at compile time, and a mismatch points at the sub-expression, not just the parameter.
- Equations in acausal components use the same language, with der(x) for a time derivative and == between the two sides.

### Units policy

Most Simulink models carry no units, so a missing unit means unknown, never an error.

- Each port and parameter is declared (unit = "rad/s", in UCUM syntax), dimensionless (unit = "1"), or unspecified (no unit key).
- Declared meets declared: different dimensions (m/s into rad) are a compile error. The same dimension at a different scale (mm into m) is also an error, with a fix that inserts a unit conversion block.
- Unspecified meets anything: no diagnostic. Units are inferred through blocks with a known unit rule (sums, delays, switches, routing, integrators, gains with a unit-bearing gain), and the editor shows inferred units in grey.
- In expressions, an unspecified operand makes the result unspecified, so nothing downstream is checked.
- Acausal domains always carry units, declared by the library, so only the signal-flow parts of a model can be unspecified.
- A project can set units.strict = true in simucraft.toml, which turns each unspecified port connected to a declared one into a warning, for teams that want full coverage.
- Imported models (.slx in v1) arrive unspecified except where the source declared units, so they compile without unit diagnostics.

## 10. Runtime architecture

### Processes

| Process | Holds and does | Runs user code |
| --- | --- | --- |
| Model service (Rust, one per open project) | Parsed project, edit commands and transactions, journal, compiler, file watcher | Never |
| Simulation workers (one or more per run or experiment) | Load a compiled model, run solvers, stream results | Yes: Python Function blocks, trusted mask hooks, native plugin blocks |
| Clients: editor, Python package, MCP server | Views and commands over JSON-RPC; never the authoritative model | Only their own |

A crash in user Python or in a solver kills one worker: that run fails with a diagnostic and the rest of the experiment continues. If the model service dies, the editor restarts it from the files plus the journal. When compiling needs the result of a trusted mask hook, the service runs the hook in a worker and caches its output by content hash.

### How a compiled model runs

- The compiler turns a model into a flat schedule: blocks in execution order, with states, outputs and parameters in contiguous arrays. In the MVP a worker interprets this schedule with one call per block method. Native blocks are compiled Rust or C, so the overhead is one function call per block, not per operation.
- Expression code (acausal residuals, expression blocks, tunable parameter expressions) compiles to compact bytecode for a small register machine.
- Jacobians come from three sources, chosen per block: symbolic derivatives from the expression engine (acausal equations and expression blocks), Jacobians that native blocks declare, and finite differences with sparsity coloring for everything else, including Python blocks. The same sources serve linearization in v1.
- A Cranelift JIT for the right-hand-side and residual functions is the planned speed-up after the MVP. It reads the same intermediate form that C code generation will use. WP0.3 measured about 9 ns per block call, so the MVP does not need the JIT (ADR 0001).
- Solvers: diffsol, a Rust library with BDF and SDIRK methods and mass-matrix DAE support, is the backend, behind the runtime's own solver trait (ADR 0001). In the WP0.3 benchmark it met tolerance on stiff ODE and DAE tests, located events to 5e-14, ran faster than CVODE on small systems, and is MIT licensed. A SUNDIALS backend can come later as a solver plugin.

### Files and the model service

- Files on disk are the source of truth. The model service holds the parsed files plus any unsaved transactions on top of them.
- The service watches the project folder. A changed file with no unsaved edits is reloaded, and clients receive files.reloaded and model.changed with the semantic diff.
- A changed file with unsaved edits is three-way merged: the base is the file as last loaded or saved, theirs is the file on disk, ours is the in-memory model. A clean merge keeps the unsaved edits on top of the new file. A conflict loads the file's version, keeps the unsaved edits as a pending change set, and raises sync.conflict for the user to resolve in the editor. Nothing is lost silently.
- Saving never overwrites a newer file: the service checks the file's hash first and merges if it changed.
- Changes that arrive together (a git checkout, a rebase, an agent writing several files) are batched: the service waits until the folder is quiet, then reloads them as one step.
- An external reload is one entry in the undo history, so undo can step back over it.

### Project trust

- A project opens restricted until the user trusts its folder, as in VS Code. A restricted project opens, compiles and simulates with installed block libraries, but nothing from the project itself runs: no mask hooks, Python Function blocks or project-local plugins. Models that need them show a diagnostic saying so.
- Trust is per folder and stored in user settings outside the project, so a repository cannot trust itself. The editor asks once; the CLI takes --trust or a list of trusted folders.
- Only the user grants trust, through the editor or the CLI. project.trust is refused over MCP.
- Installed pip packages (block libraries, solver plugins) are trusted like any Python package the user installs.

## 11. Confirmed decisions

All open points were confirmed on 2026-11-07. The table keeps the rejected alternative next to each decision, so a later change starts from the reasoning. The two solver and execution rows were settled in WP0.3; its decision record in the repository has the benchmark numbers.

| Question | Decision | Rejected alternative |
| --- | --- | --- |
| Base format | TOML 1.0 without inline tables | JSON: universal and strict, but no comments and noisier diffs |
| Table keys | Stable IDs, with names as fields | Names as keys: nicer raw diffs, but a rename touches every reference |
| ID length | 12 base32 characters (60 bits) | 10 characters (50 bits): shorter, with about a 1 in 20 million chance of any collision at 10,000 blocks |
| Expression language | Own small Python-like grammar, parsed and evaluated in Rust (Section 9) | Safe subset of Python: needs Python to compile, blocks symbolic work and C generation |
| Unit syntax | Strings such as "V\*s/rad", following UCUM conventions | Explicit dimension vectors: less readable |
| Unit literals in expressions | Brackets after a number: 24.0\[V\] | Unit constants multiplied in (24.0 \* V), which clash with parameter names |
| Unspecified units | Silent by default; units.strict turns them into warnings | Warnings by default: more coverage, but noisy for imported models |
| Model execution in the MVP | Interpreted schedule plus bytecode for expressions; Cranelift JIT after the MVP | JIT from the start: faster runs, more work before the first release |
| Solver backend | diffsol, behind the runtime's own solver trait (ADR 0001) | SUNDIALS: most proven, but a C build on every platform and slower on small systems in the benchmark; possible later as a solver plugin |
| Layout location | Separate .sclayout per model | Layout inside the model file: one file, but diagram tidying conflicts with logic changes |
