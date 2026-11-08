//! WP0.3 contract tests. Fixtures in tests/fixtures are shared with other
//! languages (Python, the editor), so they are the wire format's reference.

use serde_json::{Value, json};
use simucraft_contracts::api::{self, Command, Edit, EditResult, Outcome, Request};
use simucraft_contracts::fake;
use simucraft_contracts::ir::CompiledModel;
use simucraft_contracts::{Diagnostic, Severity};

const MSD: &str = include_str!("fixtures/mass_spring_damper.json");
const REQUESTS: &str = include_str!("fixtures/requests.json");

/// x'' = -0.5 x' - 4 x, x(0) = 1, x'(0) = 0.
fn msd_exact(t: f64) -> f64 {
    let (decay, wd) = (0.25, (4.0f64 - 0.0625).sqrt());
    (-decay * t).exp() * ((wd * t).cos() + decay / wd * (wd * t).sin())
}

#[test]
fn hand_written_compiled_model_runs_on_the_fake_runtime() {
    let model: CompiledModel = serde_json::from_str(MSD).unwrap();
    let traces = fake::run_euler(&model, &fake::library(), 1e-4, 5.0).unwrap();
    let x = &traces["x"];
    assert_eq!(x.len(), 50_001);
    for (i, &got) in x.iter().enumerate().step_by(5000) {
        let t = i as f64 * 1e-4;
        assert!((got - msd_exact(t)).abs() < 2e-3, "t={t}: {got} vs {}", msd_exact(t));
    }
}

#[test]
fn compiled_model_round_trips() {
    let model: CompiledModel = serde_json::from_str(MSD).unwrap();
    let again: CompiledModel = serde_json::from_value(serde_json::to_value(&model).unwrap()).unwrap();
    assert_eq!(model, again);
}

#[test]
fn layout_mismatch_is_reported() {
    let mut model: CompiledModel = serde_json::from_str(MSD).unwrap();
    model.blocks[0].x.len = 2;
    let err = fake::run_euler(&model, &fake::library(), 1e-3, 0.1).unwrap_err();
    assert_eq!((err.code.as_str(), err.id.as_deref()), ("R002", Some("b_00000000000v")));
}

#[test]
fn every_method_has_a_fixture_that_round_trips() {
    let raw: Vec<Value> = serde_json::from_str(REQUESTS).unwrap();
    let methods: Vec<&str> = raw.iter().map(|r| r["method"].as_str().unwrap()).collect();
    assert_eq!(methods, api::METHODS);
    for value in raw {
        let request: Request = serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{value}: {e}"));
        let mut back = serde_json::to_value(&request).unwrap();
        strip_nulls(&mut back);
        let mut want = value.clone();
        strip_nulls(&mut want);
        assert_eq!(back, want);
    }
}

fn strip_nulls(v: &mut Value) {
    if let Value::Object(map) = v {
        map.retain(|k, v| !v.is_null() || k == "D"); // `null` means "remove" in set_params
        map.values_mut().for_each(strip_nulls);
    }
}

#[test]
fn diagnostic_wire_form() {
    let d = Diagnostic::error("C010", "m/s cannot connect to rad")
        .at("speed_sensor.y")
        .id("b_e5f20hp8k1wt");
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(
        v,
        json!({"code": "C010", "severity": "error", "message": "m/s cannot connect to rad", "path": "speed_sensor.y", "id": "b_e5f20hp8k1wt"})
    );
    let mut with_fix = d.fix("insert a unit conversion block");
    with_fix.fix.as_mut().unwrap().commands.push(Command::Undo {});
    let back: Diagnostic = serde_json::from_value(serde_json::to_value(&with_fix).unwrap()).unwrap();
    assert_eq!(back, with_fix);
    assert_eq!(back.severity, Severity::Error);
}

#[test]
fn fake_service_edits_dry_runs_and_undoes() {
    let mut svc = fake::Service::default();
    let mut call = |command: Command| match svc.handle(Request::new(1, command)).outcome {
        Outcome::Result(v) => v,
        Outcome::Error(e) => panic!("{e:?}"),
    };
    let add = |name: &str, dry_run| {
        Command::BlockAdd(Edit {
            args: api::BlockAdd {
                model: "m".into(),
                type_name: "core.Gain".into(),
                name: name.into(),
                params: Default::default(),
                inputs: Default::default(),
            },
            dry_run,
        })
    };
    let added: EditResult = serde_json::from_value(call(add("a", false))).unwrap();
    assert_eq!(added.created.len(), 1);
    call(add("b", false));
    call(add("c", true)); // dry run: not applied
    let connect = Command::Connect(Edit {
        args: api::Connect {
            source: "a.y".into(),
            destination: "b.u".into(),
        },
        dry_run: false,
    });
    let result: EditResult = serde_json::from_value(call(connect)).unwrap();
    assert_eq!(result.diff[0].new, Some(json!(format!("{}.y", added.created[0]))));

    let model = call(Command::ModelGet {
        path: "m".into(),
        depth: None,
    });
    assert_eq!(model["blocks"].as_object().unwrap().len(), 2);
    call(Command::Undo {});
    let model = call(Command::ModelGet {
        path: "m".into(),
        depth: None,
    });
    assert!(
        model["blocks"]
            .as_object()
            .unwrap()
            .values()
            .all(|b| b["inputs"].as_object().unwrap().is_empty())
    );

    let unknown = svc.handle(Request::new(2, Command::Redo {}));
    assert!(matches!(unknown.outcome, Outcome::Error(e) if e.code == -32601));
}

#[test]
fn variadic_inputs_take_their_count_from_a_parameter() {
    use simucraft_contracts::block::Params;
    use simucraft_contracts::ir::ParamValue;
    let lib = simucraft_contracts::fake::library();
    let sum = lib.iter().find(|t| t.spec().type_name == "core.Sum").unwrap().spec();
    let names = |p: Params| sum.input_names(&p);
    let signs = |s: &str| Params::from([("signs".into(), ParamValue::Text(s.into()))]);
    assert_eq!(names(signs("+-+")).unwrap(), ["u1", "u2", "u3"]);
    assert!(names(Params::from([("signs".into(), ParamValue::Bool(true))])).is_err());
    // The fake Sum has no default for `signs`.
    assert!(names(Params::new()).is_err());
    let gain = lib.iter().find(|t| t.spec().type_name == "core.Gain").unwrap().spec();
    assert_eq!(gain.input_names(&Params::new()).unwrap(), ["u"]);
}
