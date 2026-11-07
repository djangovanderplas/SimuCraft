# ADR 0001: Execution model and solver backend

- Status: accepted
- Date: 2026-11-07
- Work package: WP0.3
- Spec: Section 10 ("How a compiled model runs")

## Decision

1. **Interpret the compiled model in the MVP.** A worker runs the flat
   schedule of the compiled model (`simucraft_contracts::ir`). It calls each
   block through the `Block` trait. Expressions (acausal residuals, expression
   blocks) compile to bytecode for a small register machine (WP1.8).
   A Cranelift JIT is a v1 option. It is not necessary for the MVP.
2. **Get Jacobians from three sources, per block:**
   - symbolic derivatives from the expression engine (acausal equations,
     expression blocks);
   - the `Block::jacobian` that native blocks can declare;
   - finite differences with sparsity coloring for all other blocks,
     including Python blocks.
3. **Use diffsol as the solver backend.** The runtime defines its own solver
   trait and puts diffsol behind it. SUNDIALS is not a dependency. A SUNDIALS
   backend can come later as a solver plugin (WP2.10) if a model needs it.

## Evidence

All numbers are from this repository, on the Linux cloud machine that ran
Phase 0 (release builds).

### Dispatch cost of the interpreter

`cargo run --release -p simucraft-contracts --example dispatch` runs a chain
Constant -> N x Gain on the fake runtime: two passes per step, the input gather
copy included.

| Blocks | Time per block call | Time per step |
| --- | --- | --- |
| 100 | 8.7 ns | 1.8 us |
| 1,000 | 9.1 ns | 18 us |
| 10,000 | 29 ns | 0.58 ms |

A step of a 10,000-block model costs about 0.6 ms of overhead. Real blocks do
more work than a gain, so the dispatch share goes down. This is acceptable for
the MVP. The jump at 10,000 blocks is cache misses. Do a JIT or better data
layout only if profiles of real models show the need.

### diffsol against SUNDIALS CVODE

`spikes/solver-backends` (`cargo run --release --features sundials`). Robertson
is the standard stiff chemistry test, integrated to t = 4e10 with
rtol 1e-4 and atol [1e-8, 1e-6, 1e-6]. "err/tol" is the largest error against
the reference values divided by the tolerance. Below 1 is a pass.

| Problem | Solver | Time | err/tol |
| --- | --- | --- | --- |
| Robertson, ODE form | diffsol BDF | ~115 us | 0.22 |
| Robertson, ODE form | CVODE BDF (dense LU, analytic Jacobian) | ~180 us | 0.07 |
| Robertson, DAE form (singular mass matrix) | diffsol BDF | ~120 us | 0.74 |
| Bouncing ball: root at height 0 | diffsol TSIT45 | ~4 us | event time error 5e-14 |

Other findings:

- diffsol 0.18 is MIT licensed, so it is compatible with Apache-2.0. The
  default features are pure Rust (nalgebra, faer).
- diffsol has BDF, SDIRK (TR-BDF2, ESDIRK34) and explicit Runge-Kutta
  methods, mass-matrix DAEs, root-finding events, dense output, and forward
  and adjoint sensitivities. Sensitivities are useful later for tuning.
- SUNDIALS builds from vendored sources with `sundials-sys` in about one
  minute on Linux. It needs cmake and a C compiler, and bindgen needs
  libclang. That is the build cost on Windows and macOS that we avoid.
  This spike did not test Windows.

## Risks and how we limit them

- **diffsol is pre-1.0.** Its API can change. Pin the minor version and keep
  all diffsol calls in one runtime module, behind our own trait.
- **Index-1 DAEs only.** diffsol solves semi-explicit DAEs with a mass
  matrix. Our acausal compiler (WP2.7) reduces the index to 1 first. That
  was always the plan (Pantelides with dummy derivatives).
- **Event handling at scale.** The spike tested one event. WP2.1 (zero
  crossings) must test many simultaneous and chattering events. If diffsol
  fails there, add the SUNDIALS solver plugin.

## Rejected alternatives

- **JIT from the start**: faster loops, but a large amount of work before the
  first release. The measured overhead does not justify it.
- **SUNDIALS only**: the most proven option, but a C build on each platform.
  In this test it is also slower for small systems.
