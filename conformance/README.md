# Conformance against Simulink

SimuCraft is correct when it gives the same results as Simulink for the same
model. This folder holds the reference runs and the harness that compares
them (WP1.5).

## Run the comparison

`scripts/check.sh` runs it. To run it alone:

```sh
cargo run -p simucraft-conformance
```

The harness compiles each model in `models/` and runs it two times:

- RK4 with step 1e-3. It compares the result with `ref_<name>.ode4.csv`.
- Dormand-Prince with RelTol 1e-10 and AbsTol 1e-12. It compares the result with `ref_<name>.tight.csv`.

A sample passes when `|simucraft - simulink| <= atol + rtol * |simulink|`.
The tolerances and their reasons are in one table: `CASES` in `src/main.rs`.
The harness writes the result to `report.md`, one line for each model and run.

A case that SimuCraft cannot run yet is marked "expected to fail", with the
work package that fixes it. If such a case passes, the harness fails. Then
remove the mark. Do not make a case pass with a wider tolerance.

## Add a model

1. Add a builder function to `matlab/export_references.m` and make the reference runs again.
2. Write the same model in `models/<name>.scmodel`.
3. Add two lines to `CASES`: one for `ode4` and one for `tight`.

## Make the reference runs

You need MATLAB with Simulink. The student license is sufficient.

1. In MATLAB, go to `conformance/matlab`.
2. Run `export_references('../references')`.
3. Make sure that the summary says `9 of 9 models exported`.
4. Commit `conformance/references/` on a branch and open a pull request.

The script builds every model from code. Do not edit the models by hand: the
SimuCraft tests build the same models from the same code.

## Files per model

`<name>` is the name of the builder function in the script, for example
`ref_pid_plant.tight.csv`.

| File | Content |
| --- | --- |
| `ref_<name>.slx` | The Simulink model. Later also a test input for `.slx` import. |
| `ref_<name>.ode4.csv` | Fixed-step RK4 with step 1e-3. Compare with the SimuCraft RK4 solver. |
| `ref_<name>.tight.csv` | ode45 with RelTol 1e-10 and AbsTol 1e-12, output every 1e-3. A near-exact solution for any solver. |
| `ref_<name>.json` | Solver settings, block parameters and the MATLAB version. |

CSV files have a `time` column and one column for each outport, in port
order. Values have 17 significant digits.
