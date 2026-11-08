//! WP1.8 done-when: every expression in the example files parses, unit-checks
//! and evaluates without Python, and symbolic derivatives match finite
//! differences.

use std::collections::BTreeMap;
use std::path::Path;

use simucraft_expr::{Expr, Signal, Unit, Value, check_units, compile, derive, eval, parse};
use simucraft_format::schema::{self, Design, Experiment, Model};

/// Name -> (unit, value). Values are in coherent SI units, as the evaluator uses.
type Scope = BTreeMap<String, (Option<Unit>, Value)>;

/// Block parameters that hold text, not expressions. The block library
/// (WP1.3) will declare this per block type.
const TEXT_PARAMS: [&str; 4] = ["signs", "dtype", "unit", "domain"];

fn read<T: serde::de::DeserializeOwned>(rel: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/motor_demo")
        .join(rel);
    toml_edit::de::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The model's parameters that have a literal value.
fn model_scope(model: &Model) -> Scope {
    let mut scope = Scope::new();
    for (name, p) in &model.parameters {
        let unit = p.unit.as_deref().map(|u| Unit::parse(u).unwrap());
        let value = match p.value {
            Some(schema::Value::Float(v)) => v,
            Some(schema::Value::Int(v)) => v as f64,
            _ => continue, // data files are the compiler's job
        };
        scope.insert(name.clone(), (unit, Value::Num(value * unit.map_or(1.0, |u| u.scale))));
    }
    scope
}

/// Expressions a model holds: parameter `expr` fields and block parameters
/// that are strings.
fn model_expressions(model: &Model) -> Vec<String> {
    let params = model.parameters.values().filter_map(|p| p.expr.clone());
    let blocks = model.blocks.values().flat_map(|b| {
        b.params.iter().filter_map(|(key, v)| match v {
            schema::Value::Text(s) if !TEXT_PARAMS.contains(&key.as_str()) => Some(s.clone()),
            _ => None,
        })
    });
    params.chain(blocks).collect()
}

/// Recorded signals for the metric expressions: a first-order step response
/// of the speed to 100 rad/s, and a decaying voltage command.
fn signal_scope() -> Scope {
    let t: Vec<f64> = (0..=2000).map(|i| i as f64 * 1e-3).collect();
    let speed = t.iter().map(|t| 100.0 * (1.0 - (-t / 0.1).exp())).collect();
    let volts = t.iter().map(|t| 20.0 * (-t / 0.05).exp() + 5.0).collect();
    let signal = |unit: &str, y| {
        (
            Some(Unit::parse(unit).unwrap()),
            Value::Signal(Signal { t: t.clone(), y }),
        )
    };
    Scope::from([
        ("speed".into(), signal("rad/s", speed)),
        ("voltage_cmd".into(), signal("V", volts)),
    ])
}

/// Parse, unit-check and evaluate `src`. Returns the unit and the value.
fn run(src: &str, scope: &Scope) -> (Option<Unit>, Value) {
    let e = parse(src).unwrap_or_else(|d| panic!("{src}: {d:?}"));
    let unit = check_units(&e, &|n| scope.get(n).map(|s| s.0)).unwrap_or_else(|d| panic!("{src}: {d:?}"));
    let value = eval(&e, &|n| scope.get(n).map(|s| s.1.clone())).unwrap_or_else(|d| panic!("{src}: {d:?}"));
    (unit, value)
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Num(x) => *x,
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn every_example_expression_parses_unit_checks_and_evaluates() {
    let mut checked = 0;
    for file in ["models/speed_control.scmodel", "models/subsystems/dc_motor.scmodel"] {
        let model: Model = read(file);
        let scope = model_scope(&model);
        for src in model_expressions(&model) {
            let (_, v) = run(&src, &scope);
            assert!(num(&v).is_finite(), "{file}: {src}");
            checked += 1;
        }
    }
    // Spot values with known answers.
    let motor = model_scope(&read("models/subsystems/dc_motor.scmodel"));
    let (unit, v) = run("1e-4[N*m*s/rad] * friction_scale", &motor);
    assert_eq!(unit, Some(Unit::parse("N*m*s/rad").unwrap()));
    assert!((num(&v) - 1e-4).abs() < 1e-18);
    let (unit, v) = run("J + J_load", &motor);
    assert_eq!(unit, Some(Unit::parse("kg*m2").unwrap()));
    assert!((num(&v) - 0.0121).abs() < 1e-15);

    // Metrics see signals; requirements see metrics.
    let exp: Experiment = read("experiments/motor_robustness.scexp");
    let signals = signal_scope();
    let mut metrics = Scope::new();
    for (name, src) in &exp.metrics {
        metrics.insert(name.clone(), run(src, &signals));
        checked += 1;
    }
    let overshoot = num(&metrics["overshoot"].1);
    assert!(
        overshoot.abs() < 1e-3,
        "a first-order response has no overshoot: {overshoot}"
    );
    // 2 % band of a time constant of 0.1 s: t = 0.1 * ln(50) = 0.391 s.
    assert!((num(&metrics["settling"].1) - 0.391).abs() < 2e-3);
    assert_eq!(metrics["settling"].0, Some(Unit::parse("s").unwrap()));
    assert!((num(&metrics["peak_voltage"].1) - 25.0).abs() < 1e-12);
    assert_eq!(metrics["peak_voltage"].0, Some(Unit::parse("V").unwrap()));

    let reqs = exp.requirements.unwrap();
    for (name, src) in &reqs.checks {
        let (_, v) = run(src, &metrics);
        let want = name != "voltage_ok"; // 25 V peak breaks the 24 V limit
        assert_eq!(v, Value::Bool(want), "{name}: {src}");
        checked += 1;
    }

    let design: Design = read("design/speed_loop.scdesign");
    for req in design.requirements.values() {
        if let Some(src) = &req.expr {
            assert_eq!(run(src, &signals).1, Value::Bool(false), "{src}");
            checked += 1;
        }
    }
    assert_eq!(checked, 3 + 5 + 3 + 2 + 1, "an example expression was missed");
}

#[test]
fn unit_error_points_at_the_sub_expression() {
    let e = parse("Kp * (1[m] + 2[s])").unwrap();
    let err = check_units(&e, &|_| Some(None)).unwrap_err();
    assert_eq!(err.code, "C104");
    assert_eq!(err.span.map(|s| s.col), Some(7));
    assert!(err.message.contains("1[m] + 2[s]"), "{}", err.message);
}

#[test]
fn unspecified_units_are_never_an_error() {
    let e = parse("x + 1[m] + sqrt(y)").unwrap();
    assert_eq!(check_units(&e, &|_| Some(None)), Ok(None));
}

/// Expressions that cover every differentiable operator and function.
const SMOOTH: &[&str] = &[
    "J + J_load",
    "1e-4[N*m*s/rad] * friction_scale",
    "x * y - x / y + 3 * x ** 2 - y ** x",
    "-x % 0.7 + abs(x - 2) + sqrt(y) + exp(-x) + log(y)",
    "sin(x) * cos(y) + tan(x / 3) + asin(x / 4) + acos(y / 5) + atan(x * y)",
    "atan2(y, x) + min(x, y) + max(x ** 2, y) + clip(x, -1, 1) + sign(x) * x",
    "x ** 2 if x > y else y * pi + e",
];

#[test]
fn symbolic_derivatives_match_finite_differences() {
    let at = Scope::from([
        ("x".into(), (None, Value::Num(1.3))),
        ("y".into(), (None, Value::Num(2.1))),
        ("friction_scale".into(), (None, Value::Num(1.1))),
        ("J".into(), (None, Value::Num(1e-4))),
        ("J_load".into(), (None, Value::Num(0.012))),
    ]);
    let value = |e: &Expr, scope: &Scope| num(&eval(e, &|n| scope.get(n).map(|s| s.1.clone())).unwrap());
    for src in SMOOTH {
        let e = parse(src).unwrap();
        for var in at.keys() {
            let d = derive(&e, var).unwrap_or_else(|err| panic!("{src}: {err:?}"));
            let h = 1e-6 * num(&at[var].1).abs().max(1.0);
            let shifted = |dv: f64| {
                let mut s = at.clone();
                s.get_mut(var).unwrap().1 = Value::Num(num(&at[var].1) + dv);
                value(&e, &s)
            };
            let fd = (shifted(h) - shifted(-h)) / (2.0 * h);
            let sym = value(&d, &at);
            assert!(
                (sym - fd).abs() <= 1e-6 * fd.abs().max(1.0),
                "d({src})/d{var}: {sym} vs {fd}"
            );
        }
    }
}

#[test]
fn bytecode_matches_the_tree_walker() {
    let names = ["x", "y", "friction_scale", "J", "J_load"];
    let inputs = [1.3, 2.1, 1.1, 1e-4, 0.012];
    for src in SMOOTH {
        let e = parse(src).unwrap();
        let program = compile(&e, &names).unwrap_or_else(|err| panic!("{src}: {err:?}"));
        let want = num(&eval(&e, &|n| {
            names.iter().position(|m| *m == n).map(|i| Value::Num(inputs[i]))
        })
        .unwrap());
        assert_eq!(program.eval(&inputs), want, "{src}");
    }
}

#[test]
fn precedence_follows_python_and_bad_text_is_a_diagnostic() {
    let value = |src: &str| eval(&parse(src).unwrap(), &|n| (n == "x").then_some(Value::Num(3.0))).unwrap();
    assert_eq!(value("-x ** 2"), Value::Num(-9.0));
    assert_eq!(value("2 ** 3 ** 2"), Value::Num(512.0));
    assert_eq!(value("-7 % 3"), Value::Num(2.0));
    assert_eq!(value("1 if x > 2 and not x > 5 else 0"), Value::Num(1.0));
    assert_eq!(
        value("[[1, 0], [0, 1]][1][1] + interp(x, [0, 10], [0, 1])"),
        Value::Num(1.3)
    );
    assert_eq!(value("2[ms] + 1[s]"), Value::Num(1.002));
    for (src, code, col) in [
        ("1 +", "C101", 4),
        ("1 < x < 2", "C101", 7),
        ("x.y", "C101", 2),
        ("2[furlong]", "C106", 3),
        ("foo(1)", "C103", 1),
        ("import", "C102", 1),
    ] {
        let err = parse(src).and_then(|e| check_units(&e, &|n| (n == "x").then_some(None)).map(|_| ()));
        let err = err.unwrap_err();
        assert_eq!(
            (err.code.as_str(), err.span.map(|s| s.col)),
            (code, Some(col)),
            "{src}: {err:?}"
        );
    }
}
