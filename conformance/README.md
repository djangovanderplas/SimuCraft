# Conformance against Simulink

SimuCraft is correct when it gives the same results as Simulink for the same
model. This folder holds the reference runs and, later, the harness that
compares them (WP1.5).

## Make the reference runs

You need MATLAB with Simulink. The student license is sufficient.

1. In MATLAB, go to `conformance/matlab`.
2. Run `export_references('../references')`.
3. Make sure that the summary says `9 of 9 models exported`.
4. Commit `conformance/references/` on a branch and open a pull request.

The script builds every model from code. Do not edit the models by hand: the
SimuCraft tests build the same models from the same code.

## Files per model

| File | Content |
| --- | --- |
| `<name>.slx` | The Simulink model. Later also a test input for `.slx` import. |
| `<name>.ode4.csv` | Fixed-step RK4 with step 1e-3. Compare with the SimuCraft RK4 solver. |
| `<name>.tight.csv` | ode45 with RelTol 1e-10 and AbsTol 1e-12, output every 1e-3. A near-exact solution for any solver. |
| `<name>.json` | Solver settings, block parameters and the MATLAB version. |

CSV files have a `time` column and one column for each outport, in port
order. Values have 17 significant digits.
