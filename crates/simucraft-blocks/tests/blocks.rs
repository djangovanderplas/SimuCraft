//! WP1.3 done-when: each block passes its contract tests. Continuous blocks
//! run on `fake::run_euler`; the discrete PID is driven through its `Block`
//! methods directly.

use serde_json::{Value, json};
use simucraft_blocks::library;
use simucraft_contracts::block::{BlockType, Ctx, Params, SampleTime};
use simucraft_contracts::fake;
use simucraft_contracts::ir::{CompiledModel, ParamValue};

const DT: f64 = 1e-4;

fn find(name: &str) -> Box<dyn BlockType> {
    library().into_iter().find(|t| t.spec().type_name == name).unwrap()
}

/// A compiled model from `(type, params, inputs, outputs, x)` blocks, all at
/// rate 0 (continuous). The signal `y` (slot `logged`) is logged.
fn ir(signals: usize, blocks: &[(&str, Value, Value, Value, [usize; 2])], logged: usize) -> CompiledModel {
    let nx = blocks.iter().map(|b| b.4[1]).sum::<usize>();
    let blocks: Vec<Value> = blocks
        .iter()
        .enumerate()
        .map(|(i, (ty, params, inputs, outputs, x))| {
            json!({"id": format!("b_{i:012}"), "type_name": ty, "params": params,
                   "inputs": inputs, "outputs": outputs, "x": x, "rate": 0})
        })
        .collect();
    serde_json::from_value(json!({
        "format": "simucraft.ir/0.1", "signals": signals, "continuous_states": nx,
        "rates": [{"kind": "continuous"}], "blocks": blocks, "logged": {"y": [logged, 1]}
    }))
    .unwrap()
}

/// Run `model` with Euler to `t_end` and return the logged `y`.
fn run(model: &CompiledModel, t_end: f64) -> Vec<f64> {
    fake::run_euler(model, &library(), DT, t_end)
        .unwrap()
        .remove("y")
        .unwrap()
}

fn at(y: &[f64], t: f64) -> f64 {
    y[(t / DT).round() as usize]
}

#[test]
fn every_block_type_builds_with_its_defaults_and_fits_its_spec() {
    let names: Vec<String> = library().iter().map(|t| t.spec().type_name.clone()).collect();
    for want in [
        "core.Constant",
        "core.Step",
        "core.Gain",
        "core.Sum",
        "core.Integrator",
        "core.TransferFcn",
        "core.PID",
        "core.Saturation",
        "core.Scope",
        "core.Inport",
        "core.Outport",
    ] {
        assert!(names.iter().any(|n| n == want), "missing {want}");
    }
    for ty in library() {
        let spec = ty.spec();
        let ports = spec.input_names(&Params::new()).unwrap();
        let widths = vec![1; ports.len()];
        let (_, layout) = ty
            .build(&Params::new(), &widths, SampleTime::Continuous)
            .unwrap_or_else(|d| panic!("{}: {d:?}", spec.type_name));
        assert_eq!(layout.inputs.len(), ports.len(), "{}", spec.type_name);
        assert_eq!(layout.outputs.len(), spec.outputs.len(), "{}", spec.type_name);
        assert_eq!(layout.feedthrough.len(), ports.len(), "{}", spec.type_name);
        assert!(
            spec.params.iter().all(|p| p.default.is_some()),
            "{}: a parameter has no default",
            spec.type_name
        );
    }
}

#[test]
fn bad_parameters_are_diagnostics() {
    let text = Params::from([("k".into(), ParamValue::Text("x".into()))]);
    assert_eq!(
        find("core.Gain")
            .build(&text, &[1], SampleTime::Continuous)
            .err()
            .unwrap()
            .code,
        "C201"
    );
    let signs = Params::from([("signs".into(), ParamValue::Text("+-+".into()))]);
    assert_eq!(
        find("core.Sum")
            .build(&signs, &[1, 1], SampleTime::Continuous)
            .err()
            .unwrap()
            .code,
        "C202"
    );
    let improper = Params::from([
        ("num".into(), ParamValue::Vector(vec![1.0, 0.0, 0.0])),
        ("den".into(), ParamValue::Vector(vec![1.0, 1.0])),
    ]);
    assert_eq!(
        find("core.TransferFcn")
            .build(&improper, &[1], SampleTime::Continuous)
            .err()
            .unwrap()
            .code,
        "C201"
    );
}

#[test]
fn constant_gain_and_sum() {
    // y = 3 * 2 - 1 = 5
    let m = ir(
        4,
        &[
            (
                "core.Constant",
                json!({"value": 2.0}),
                json!([]),
                json!([[0, 1]]),
                [0, 0],
            ),
            (
                "core.Constant",
                json!({"value": 1.0}),
                json!([]),
                json!([[1, 1]]),
                [0, 0],
            ),
            ("core.Gain", json!({"k": 3.0}), json!([[0, 1]]), json!([[2, 1]]), [0, 0]),
            (
                "core.Sum",
                json!({"signs": "+-"}),
                json!([[2, 1], [1, 1]]),
                json!([[3, 1]]),
                [0, 0],
            ),
        ],
        3,
    );
    assert!(run(&m, 0.01).iter().all(|y| *y == 5.0));
}

#[test]
fn step_switches_at_its_time() {
    let m = ir(
        1,
        &[(
            "core.Step",
            json!({"time": 0.5, "initial": -1.0, "final": 2.0}),
            json!([]),
            json!([[0, 1]]),
            [0, 0],
        )],
        0,
    );
    let y = run(&m, 1.0);
    assert_eq!((at(&y, 0.4999), at(&y, 0.5), at(&y, 1.0)), (-1.0, 2.0, 2.0));
}

#[test]
fn integrator_of_a_constant_is_a_ramp() {
    let m = ir(
        2,
        &[
            (
                "core.Constant",
                json!({"value": 2.0}),
                json!([]),
                json!([[0, 1]]),
                [0, 0],
            ),
            (
                "core.Integrator",
                json!({"x0": 1.0}),
                json!([[0, 1]]),
                json!([[1, 1]]),
                [0, 1],
            ),
        ],
        1,
    );
    assert!((at(&run(&m, 1.0), 1.0) - 3.0).abs() < 1e-9);
}

/// Step response of a transfer function driven by a constant 1.
fn tf_step(num: Value, den: Value, states: usize, t_end: f64) -> Vec<f64> {
    let m = ir(
        2,
        &[
            (
                "core.Constant",
                json!({"value": 1.0}),
                json!([]),
                json!([[0, 1]]),
                [0, 0],
            ),
            (
                "core.TransferFcn",
                json!({"num": num, "den": den}),
                json!([[0, 1]]),
                json!([[1, 1]]),
                [0, states],
            ),
        ],
        1,
    );
    run(&m, t_end)
}

#[test]
fn transfer_fcn_matches_its_step_response() {
    let y = tf_step(json!([1.0]), json!([2.0, 2.0]), 1, 3.0); // 1 / (2s + 2)
    for t in [0.5, 1.0, 3.0] {
        assert!((at(&y, t) - 0.5 * (1.0 - (-t).exp())).abs() < 1e-4, "t={t}");
    }
    // (s + 3) / (s^2 + 3s + 2): y = 3/2 - 2 e^-t + 1/2 e^-2t
    let y = tf_step(json!([1.0, 3.0]), json!([1.0, 3.0, 2.0]), 2, 3.0);
    for t in [0.2f64, 1.0, 3.0] {
        let want = 1.5 - 2.0 * (-t).exp() + 0.5 * (-2.0 * t).exp();
        assert!((at(&y, t) - want).abs() < 1e-3, "t={t}: {} vs {want}", at(&y, t));
    }
    // Proper (not strictly proper): (s + 2) / (s + 1) has direct feedthrough: y = 2 - e^-t.
    let y = tf_step(json!([1.0, 2.0]), json!([1.0, 1.0]), 1, 1.0);
    assert!((at(&y, 0.0) - 1.0).abs() < 1e-12);
    assert!((at(&y, 1.0) - (2.0 - (-1.0f64).exp())).abs() < 1e-4);
}

#[test]
fn continuous_pid_on_a_constant_error() {
    // y = P + I t + D N e^(-N t), for e = 1.
    let (p, i, d, n) = (2.0, 3.0, 0.1, 10.0f64);
    let m = ir(
        2,
        &[
            (
                "core.Constant",
                json!({"value": 1.0}),
                json!([]),
                json!([[0, 1]]),
                [0, 0],
            ),
            (
                "core.PID",
                json!({"P": p, "I": i, "D": d, "N": n}),
                json!([[0, 1]]),
                json!([[1, 1]]),
                [0, 2],
            ),
        ],
        1,
    );
    let y = run(&m, 1.0);
    for t in [0.0f64, 0.1, 1.0] {
        let want = p + i * t + d * n * (-n * t).exp();
        assert!((at(&y, t) - want).abs() < 1e-3, "t={t}: {} vs {want}", at(&y, t));
    }
}

#[test]
fn discrete_pid_uses_forward_euler_at_its_sample_time() {
    let params = Params::from([
        ("P".into(), ParamValue::Scalar(2.0)),
        ("I".into(), ParamValue::Scalar(3.0)),
        ("D".into(), ParamValue::Scalar(0.1)),
        ("N".into(), ParamValue::Scalar(10.0)),
    ]);
    let rate = SampleTime::Discrete {
        period: 0.1,
        offset: 0.0,
    };
    let (mut block, layout) = find("core.PID").build(&params, &[1], rate).unwrap();
    assert_eq!(
        layout.sample_time,
        SampleTime::Discrete {
            period: 0.1,
            offset: 0.0
        }
    );
    assert_eq!((layout.continuous_states, layout.discrete_states), (0, 2));
    let mut xd = [0.0; 2];
    block.initialize(&mut [], &mut xd);
    let mut y = [0.0];
    let mut outputs = Vec::new();
    for k in 0..3 {
        let mut cx = Ctx {
            t: k as f64 * 0.1,
            major: true,
            u: &[1.0],
            u_off: &[0, 1],
            y: &mut y,
            y_off: &[0, 1],
            x: &[],
            dx: &mut [],
            xd: &mut xd,
            z: &[],
            r: &mut [],
            zc: &mut [],
        };
        block.outputs(&mut cx);
        block.update(&mut cx);
        outputs.push(y[0]);
    }
    // k=0: 2 + 0 + 0.1*10*(1-0) = 3; k=1: integral 0.1, filter 1 -> 2.3; k=2: integral 0.2 -> 2.6.
    let want = [3.0, 2.3, 2.6];
    assert!(
        outputs.iter().zip(want).all(|(y, w)| (y - w).abs() < 1e-12),
        "{outputs:?}"
    );
}

#[test]
fn saturation_clamps_both_sides() {
    for (value, want) in [(5.0, 1.0), (-5.0, -0.5), (0.25, 0.25)] {
        let m = ir(
            2,
            &[
                (
                    "core.Constant",
                    json!({"value": value}),
                    json!([]),
                    json!([[0, 1]]),
                    [0, 0],
                ),
                (
                    "core.Saturation",
                    json!({"upper": 1.0, "lower": -0.5}),
                    json!([[0, 1]]),
                    json!([[1, 1]]),
                    [0, 0],
                ),
            ],
            1,
        );
        assert_eq!(run(&m, 0.001)[0], want);
    }
}

#[test]
fn sinks_and_ports_run_and_change_nothing() {
    let m = ir(
        2,
        &[
            ("core.Inport", json!({}), json!([]), json!([[0, 1]]), [0, 0]),
            (
                "core.Constant",
                json!({"value": 4.0}),
                json!([]),
                json!([[1, 1]]),
                [0, 0],
            ),
            ("core.Scope", json!({}), json!([[1, 1]]), json!([]), [0, 0]),
            ("core.Outport", json!({}), json!([[1, 1]]), json!([]), [0, 0]),
        ],
        1,
    );
    assert!(run(&m, 0.001).iter().all(|y| *y == 4.0));
}
