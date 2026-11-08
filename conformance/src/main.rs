//! The WP1.5 conformance harness. It compiles each model in `models/`, runs
//! it with SimuCraft, and compares the outports with the Simulink reference
//! runs in `references/`:
//!
//! - RK4 with step 1e-3 against `ref_<name>.ode4.csv` (Simulink ode4),
//! - Dormand-Prince against `ref_<name>.tight.csv` (Simulink ode45, RelTol 1e-10).
//!
//! Run it with `cargo run -p simucraft-conformance`. It prints the report and
//! writes it to `report.md`. The exit code is 1 if a case fails that must
//! pass, or if a case passes that is marked "expected to fail".

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use simucraft_contracts::Severity;
use simucraft_runtime::{DormandPrince, Rk4, Simulation, Solver};

/// One comparison: a model, a reference run, and its tolerance.
struct Case {
    model: &'static str,
    /// `ode4` or `tight`.
    run: &'static str,
    /// A sample passes when `|simucraft - simulink| <= atol + rtol * |simulink|`.
    atol: f64,
    rtol: f64,
    /// Why this tolerance is correct for this case.
    why: &'static str,
    /// The reason and the work package that fixes it, if SimuCraft cannot
    /// run this case yet.
    xfail: Option<&'static str>,
}

const fn case(
    model: &'static str,
    run: &'static str,
    atol: f64,
    rtol: f64,
    why: &'static str,
    xfail: Option<&'static str>,
) -> Case {
    Case {
        model,
        run,
        atol,
        rtol,
        why,
        xfail,
    }
}

/// Simulink and SimuCraft use the same RK4 method and the same step, so only
/// the order of floating-point operations differs.
const SAME_RK4: &str = "same RK4 and the same step: only rounding differs";
/// Both runs are near-exact (rtol 1e-10): the Simulink run is within 2e-15 of
/// the analytic mass-spring-damper. The global error of a run grows past its
/// local tolerance, so the limit is 100 times rtol. RK4 at 1e-3 is 4e-13 from
/// exact, so this limit cannot tell RK4 from exact; the ode4 cases do that.
const BOTH_TIGHT: &str = "both near-exact (rtol 1e-10); global error can grow to 100 times the local tolerance";
/// No continuous states: the outputs are the same arithmetic at each time.
const NO_STATES: &str = "no continuous states: the same arithmetic at each output time";

const NO_SINE: &str = "WP2.3: no `core.Sine` block";
const NO_LIMITS: &str = "WP2.3 and WP2.1: no `core.Sine`, no integrator limits (they need zero crossings)";
const NO_MULTIRATE: &str = "WP2.3 and WP2.1: no Sine, Zero-Order Hold, Rate Transition or Unit Delay block";

/// The tolerances. Every reference file has exactly one case here.
const CASES: &[Case] = &[
    case("basic_sources", "ode4", 1e-12, 0.0, NO_STATES, None),
    case("basic_sources", "tight", 1e-12, 0.0, NO_STATES, None),
    case("integrator_ramp", "ode4", 1e-12, 1e-12, SAME_RK4, None),
    case("integrator_ramp", "tight", 1e-8, 1e-8, BOTH_TIGHT, None),
    case("mass_spring_damper", "ode4", 1e-12, 1e-12, SAME_RK4, None),
    case("mass_spring_damper", "tight", 1e-8, 1e-8, BOTH_TIGHT, None),
    case("transfer_fcn_step", "ode4", 1e-12, 1e-12, SAME_RK4, None),
    case("transfer_fcn_step", "tight", 1e-8, 1e-8, BOTH_TIGHT, None),
    case("pid_plant", "ode4", 1e-12, 1e-12, SAME_RK4, None),
    case("pid_plant", "tight", 1e-8, 1e-8, BOTH_TIGHT, None),
    case("saturation_sine", "ode4", 1e-12, 0.0, NO_STATES, Some(NO_SINE)),
    case("saturation_sine", "tight", 1e-12, 0.0, NO_STATES, Some(NO_SINE)),
    case("saturating_integrator", "ode4", 1e-12, 1e-12, SAME_RK4, Some(NO_LIMITS)),
    case(
        "saturating_integrator",
        "tight",
        1e-8,
        1e-8,
        BOTH_TIGHT,
        Some(NO_LIMITS),
    ),
    case("discrete_pid_1khz", "ode4", 1e-12, 1e-12, SAME_RK4, None),
    case("discrete_pid_1khz", "tight", 1e-8, 1e-8, BOTH_TIGHT, None),
    case("multirate", "ode4", 1e-12, 0.0, NO_STATES, Some(NO_MULTIRATE)),
    case("multirate", "tight", 1e-12, 0.0, NO_STATES, Some(NO_MULTIRATE)),
];

/// Output times in the reference runs and in our runs.
const OUTPUT_STEP: f64 = 1e-3;

/// A table of signals: `time` and one column per outport.
struct Table {
    names: Vec<String>,
    t: Vec<f64>,
    /// One vector per name.
    cols: Vec<Vec<f64>>,
}

/// The sample that is nearest to its tolerance limit.
#[derive(Debug)]
struct Worst {
    signal: String,
    t: f64,
    err: f64,
    /// `err / (atol + rtol * |reference|)`: above 1 fails.
    ratio: f64,
}

fn read_csv(path: &Path) -> Result<Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("empty CSV")?.split(',').collect();
    if header.first() != Some(&"time") {
        return Err(format!("{}: the first column is not `time`", path.display()));
    }
    let mut table = Table {
        names: header[1..].iter().map(|s| s.to_string()).collect(),
        t: Vec::new(),
        cols: vec![Vec::new(); header.len() - 1],
    };
    for (n, line) in lines.enumerate() {
        let row: Vec<f64> = line
            .split(',')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{} line {}: {e}", path.display(), n + 2))?;
        if row.len() != header.len() {
            return Err(format!("{} line {}: wrong number of columns", path.display(), n + 2));
        }
        table.t.push(row[0]);
        for (col, v) in table.cols.iter_mut().zip(&row[1..]) {
            col.push(*v);
        }
    }
    Ok(table)
}

/// Compile and run `models/<model>.scmodel` and record the outports that
/// `reference` has, at every output time up to its last time.
fn simulate(dir: &Path, c: &Case, reference: &Table) -> Result<Table, String> {
    let library = simucraft_blocks::library();
    let path = dir.join("models").join(format!("{}.scmodel", c.model));
    let (model, diagnostics) = simucraft_compiler::compile(&path, &library, &|p| std::fs::read_to_string(p).ok());
    let model = model.ok_or_else(|| {
        let errors: Vec<String> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        format!("does not compile: {}", errors.join("; "))
    })?;
    let mut slots = Vec::new();
    for name in &reference.names {
        match model.outports.get(name) {
            Some(s) if s.len == 1 => slots.push(s.start),
            _ => return Err(format!("no scalar outport `{name}`")),
        }
    }
    let mut solver: Box<dyn Solver> = match c.run {
        "ode4" => Box::new(Rk4 { h: OUTPUT_STEP }),
        // The tolerances of the Simulink ode45 run.
        _ => Box::new(DormandPrince::new(1e-10, 1e-12)),
    };
    let t_end = *reference.t.last().ok_or("the reference has no rows")?;
    let mut sim = Simulation::new(&model, &library).map_err(|d| format!("{} {}", d.code, d.message))?;
    let mut out = Table {
        names: reference.names.clone(),
        t: Vec::new(),
        cols: vec![Vec::new(); slots.len()],
    };
    sim.stream(solver.as_mut(), t_end, OUTPUT_STEP, &mut |t, signals| {
        out.t.push(t);
        for (col, &s) in out.cols.iter_mut().zip(&slots) {
            col.push(signals[s]);
        }
    })
    .map_err(|d| format!("{} {}", d.code, d.message))?;
    Ok(out)
}

/// Compare on the shared time grid. Both tables must have the same times.
fn compare(ours: &Table, reference: &Table, atol: f64, rtol: f64) -> Result<Worst, String> {
    if ours.t.len() != reference.t.len() {
        return Err(format!(
            "{} output times, the reference has {}",
            ours.t.len(),
            reference.t.len()
        ));
    }
    if let Some(i) = (0..ours.t.len()).find(|&i| (ours.t[i] - reference.t[i]).abs() > 1e-9 * OUTPUT_STEP) {
        return Err(format!("time grids differ at t = {}", reference.t[i]));
    }
    let mut worst = Worst {
        signal: String::new(),
        t: 0.0,
        err: 0.0,
        ratio: -1.0,
    };
    for (k, name) in reference.names.iter().enumerate() {
        for (i, (&a, &r)) in ours.cols[k].iter().zip(&reference.cols[k]).enumerate() {
            let err = (a - r).abs();
            let ratio = err / (atol + rtol * r.abs());
            // A NaN in our run must fail, so NaN counts as infinitely bad.
            let ratio = if ratio.is_nan() { f64::INFINITY } else { ratio };
            if ratio > worst.ratio {
                worst = Worst {
                    signal: name.clone(),
                    t: reference.t[i],
                    err,
                    ratio,
                };
            }
        }
    }
    Ok(worst)
}

fn main() -> ExitCode {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut report = String::from(
        "# Conformance report\n\n\
         Made by `cargo run -p simucraft-conformance` (WP1.5). The worst sample is the one\n\
         nearest to its limit `atol + rtol * |simulink|`; \"of tol\" is its error divided by\n\
         that limit, so 1 or more fails. The tolerances and their reasons are in\n\
         `conformance/src/main.rs` (`CASES`).\n\n\
         | Model | Run | Result | Worst error | At t [s] | Signal | Of tol | atol | rtol | Note |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    let mut unexpected = 0;
    for c in CASES {
        let file = dir.join("references").join(format!("ref_{}.{}.csv", c.model, c.run));
        let result = read_csv(&file).and_then(|r| {
            let ours = simulate(dir, c, &r)?;
            compare(&ours, &r, c.atol, c.rtol)
        });
        let passed = matches!(&result, Ok(w) if w.ratio <= 1.0);
        let label = match (passed, c.xfail.is_some()) {
            (true, false) => "pass",
            (false, false) => "**FAIL**",
            (true, true) => "**UNEXPECTED PASS**: remove the expected fail",
            (false, true) => "expected fail",
        };
        if passed == c.xfail.is_some() {
            unexpected += 1;
        }
        let note = match (&result, c.xfail) {
            (Err(e), Some(x)) => format!("{x}. {e}"),
            (Err(e), None) => e.clone(),
            (Ok(_), Some(x)) => x.to_string(),
            (Ok(_), None) => c.why.to_string(),
        };
        let cells = match &result {
            Ok(w) => format!("{:.2e} | {:.3} | {} | {:.2}", w.err, w.t, w.signal, w.ratio),
            Err(_) => "- | - | - | -".into(),
        };
        let _ = writeln!(
            report,
            "| {} | {} | {label} | {cells} | {:.0e} | {:.0e} | {note} |",
            c.model, c.run, c.atol, c.rtol
        );
    }
    print!("{report}");
    if let Err(e) = std::fs::write(dir.join("report.md"), &report) {
        eprintln!("cannot write report.md: {e}");
        return ExitCode::FAILURE;
    }
    if unexpected > 0 {
        eprintln!("conformance: {unexpected} unexpected result(s)");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(t: &[f64], y: &[f64]) -> Table {
        Table {
            names: vec!["y".into()],
            t: t.to_vec(),
            cols: vec![y.to_vec()],
        }
    }

    #[test]
    fn compare_finds_the_sample_nearest_to_its_limit() {
        let r = table(&[0.0, 0.001, 0.002], &[1.0, 100.0, 1.0]);
        let ours = table(&[0.0, 0.001, 0.002], &[1.0, 100.5, 1.2]);
        let w = compare(&ours, &r, 0.1, 0.01).unwrap();
        // 0.5 / (0.1 + 1.0) is below 0.2 / (0.1 + 0.01).
        assert_eq!(w.t, 0.002);
        assert!(w.ratio > 1.0);
        let nan = table(&[0.0, 0.001, 0.002], &[1.0, f64::NAN, 1.0]);
        assert!(compare(&nan, &r, 0.1, 0.01).unwrap().ratio.is_infinite());
        assert!(compare(&table(&[0.0], &[1.0]), &r, 0.1, 0.01).is_err());
    }

    #[test]
    fn every_reference_file_has_one_case() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("references");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.ends_with(".csv"))
            .collect();
        files.sort();
        let mut cases: Vec<String> = CASES.iter().map(|c| format!("ref_{}.{}.csv", c.model, c.run)).collect();
        cases.sort();
        assert_eq!(files, cases);
    }
}
