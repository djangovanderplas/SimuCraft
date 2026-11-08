# Conformance report

Made by `cargo run -p simucraft-conformance` (WP1.5). The worst sample is the one
nearest to its limit `atol + rtol * |simulink|`; "of tol" is its error divided by
that limit, so 1 or more fails. The tolerances and their reasons are in
`conformance/src/main.rs` (`CASES`).

| Model | Run | Result | Worst error | At t [s] | Signal | Of tol | atol | rtol | Note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| basic_sources | ode4 | pass | 0.00e0 | 0.000 | y | 0.00 | 1e-12 | 0e0 | no continuous states: the same arithmetic at each output time |
| basic_sources | tight | pass | 0.00e0 | 0.000 | y | 0.00 | 1e-12 | 0e0 | no continuous states: the same arithmetic at each output time |
| integrator_ramp | ode4 | pass | 4.48e-13 | 4.999 | x | 0.07 | 1e-12 | 1e-12 | same RK4 and the same step: only rounding differs |
| integrator_ramp | tight | pass | 2.81e-13 | 4.000 | x | 0.00 | 1e-8 | 1e-8 | both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance |
| mass_spring_damper | ode4 | pass | 3.94e-15 | 7.122 | v | 0.00 | 1e-12 | 1e-12 | same RK4 and the same step: only rounding differs |
| mass_spring_damper | tight | pass | 1.94e-15 | 6.156 | v | 0.00 | 1e-8 | 1e-8 | both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance |
| transfer_fcn_step | ode4 | pass | 4.00e-15 | 2.965 | y | 0.00 | 1e-12 | 1e-12 | same RK4 and the same step: only rounding differs |
| transfer_fcn_step | tight | pass | 1.44e-15 | 6.094 | y | 0.00 | 1e-8 | 1e-8 | both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance |
| pid_plant | ode4 | pass | 3.16e-15 | 1.240 | u | 0.00 | 1e-12 | 1e-12 | same RK4 and the same step: only rounding differs |
| pid_plant | tight | pass | 1.58e-10 | 0.149 | u | 0.01 | 1e-8 | 1e-8 | both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance |
| saturation_sine | ode4 | expected fail | - | - | - | - | 1e-12 | 0e0 | WP2.3: no `core.Sine` block. does not compile: C001 unknown block type `core.Sine`; C002 `sine` did not compile; C002 `sine` did not compile |
| saturation_sine | tight | expected fail | - | - | - | - | 1e-12 | 0e0 | WP2.3: no `core.Sine` block. does not compile: C001 unknown block type `core.Sine`; C002 `sine` did not compile; C002 `sine` did not compile |
| saturating_integrator | ode4 | expected fail | - | - | - | - | 1e-12 | 1e-12 | WP2.3 and WP2.1: no `core.Sine`, no integrator limits (they need zero crossings). does not compile: C001 unknown block type `core.Sine`; C006 `core.Integrator` has no parameter `lower`; C006 `core.Integrator` has no parameter `upper`; C002 `int` did not compile |
| saturating_integrator | tight | expected fail | - | - | - | - | 1e-8 | 1e-8 | WP2.3 and WP2.1: no `core.Sine`, no integrator limits (they need zero crossings). does not compile: C001 unknown block type `core.Sine`; C006 `core.Integrator` has no parameter `lower`; C006 `core.Integrator` has no parameter `upper`; C002 `int` did not compile |
| discrete_pid_1khz | ode4 | pass | 1.33e-15 | 1.340 | u | 0.00 | 1e-12 | 1e-12 | same RK4 and the same step: only rounding differs |
| discrete_pid_1khz | tight | pass | 1.44e-15 | 2.257 | u | 0.00 | 1e-8 | 1e-8 | both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance |
| multirate | ode4 | expected fail | - | - | - | - | 1e-12 | 0e0 | WP2.3 and WP2.1: no Sine, Zero-Order Hold, Rate Transition or Unit Delay block. does not compile: C001 unknown block type `core.Sine`; C001 unknown block type `core.ZeroOrderHold`; C001 unknown block type `core.RateTransition`; C001 unknown block type `core.UnitDelay`; C002 `zoh_fast` did not compile; C002 `delay` did not compile |
| multirate | tight | expected fail | - | - | - | - | 1e-12 | 0e0 | WP2.3 and WP2.1: no Sine, Zero-Order Hold, Rate Transition or Unit Delay block. does not compile: C001 unknown block type `core.Sine`; C001 unknown block type `core.ZeroOrderHold`; C001 unknown block type `core.RateTransition`; C001 unknown block type `core.UnitDelay`; C002 `zoh_fast` did not compile; C002 `delay` did not compile |
