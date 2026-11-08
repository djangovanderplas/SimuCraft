//! WP1.4: the PID example is built and saved through Model API commands.

use serde_json::{Value, json};
use simucraft_contracts::api::{Outcome, Request};
use simucraft_format::{Kind, check};
use simucraft_service::Service;

fn call(svc: &mut Service, method: &str, params: Value) -> Result<Value, Value> {
    let request: Request =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .unwrap_or_else(|e| panic!("{method}: {e}"));
    match svc.handle(request).outcome {
        Outcome::Result(v) => Ok(v),
        Outcome::Error(e) => Err(serde_json::to_value(e).unwrap()),
    }
}

fn ok(svc: &mut Service, method: &str, params: Value) -> Value {
    call(svc, method, params).unwrap_or_else(|e| panic!("{method}: {e:#}"))
}

fn add(svc: &mut Service, ty: &str, name: &str, params: Value) -> String {
    let r = ok(
        svc,
        "block.add",
        json!({"model": "models/pid.scmodel", "type": ty, "name": name, "params": params}),
    );
    r["created"][0].as_str().unwrap().to_string()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("simucraft-service-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn pid_example_is_built_and_saved() {
    let dir = scratch("pid");
    let mut svc = Service::default();
    ok(
        &mut svc,
        "project.create",
        json!({"path": dir.to_str().unwrap(), "name": "pid"}),
    );

    ok(
        &mut svc,
        "param.set",
        json!({"model": "models/pid.scmodel", "name": "Kp", "value": 2.0, "tunable": true}),
    );
    ok(
        &mut svc,
        "param.set",
        json!({"model": "models/pid.scmodel", "name": "Ki", "expr": "Kp * 1.5"}),
    );
    ok(&mut svc, "tx.begin", json!({"label": "Build the loop"}));
    add(&mut svc, "core.Constant", "ref", json!({"value": 1.0}));
    add(&mut svc, "core.Sum", "error", json!({"signs": "+-"}));
    let pid = add(&mut svc, "core.PID", "pid", json!({"P": "Kp", "I": "Ki", "D": 0.0}));
    add(
        &mut svc,
        "core.TransferFcn",
        "plant",
        json!({"num": [1.0], "den": [0.5, 1.0]}),
    );
    add(&mut svc, "core.Outport", "y", json!({}));
    for (src, dst) in [
        ("ref.y", "error.u1"),
        ("plant.y", "error.u2"),
        ("error.y", "pid.u"),
        ("pid.y", "plant.u"),
        ("plant.y", "y.u"),
    ] {
        ok(&mut svc, "connect", json!({"source": src, "destination": dst}));
    }
    ok(&mut svc, "tx.commit", json!({}));
    ok(
        &mut svc,
        "signal.set",
        json!({"port": "error.y", "name": "e", "log": true}),
    );

    // The diff names the change; a dry run changes nothing.
    let dry = ok(
        &mut svc,
        "block.set_params",
        json!({"target": "pid", "params": {"D": 0.1}, "dry_run": true}),
    );
    assert_eq!(dry["diff"][0]["path"], format!("blocks.{pid}.params.D"));
    assert_eq!(
        (dry["diff"][0]["old"].clone(), dry["diff"][0]["new"].clone()),
        (json!(0.0), json!(0.1))
    );
    let model = ok(&mut svc, "model.get", json!({"path": "models/pid.scmodel"}));
    assert_eq!(model["model"]["blocks"][&pid]["params"]["D"], json!(0.0));

    // Undo takes back the whole transaction or one edit.
    ok(&mut svc, "block.rename", json!({"target": "plant", "name": "motor"}));
    ok(&mut svc, "undo", json!({}));
    assert_eq!(
        ok(&mut svc, "find", json!({"query": "plant"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(&mut svc, "redo", json!({}));
    ok(&mut svc, "undo", json!({}));

    let status = ok(&mut svc, "project.status", json!({}));
    assert_eq!(status["unsaved"], json!(["models/pid.scmodel"]));
    let saved = ok(&mut svc, "project.save", json!({}));
    assert_eq!(saved["written"], json!(["models/pid.scmodel"]));

    let text = std::fs::read_to_string(dir.join("models/pid.scmodel")).unwrap();
    assert_eq!(check(Kind::Model, &text), []);
    assert!(text.contains(&format!("u = \"{pid}.y\"")), "{text}");
    let manifest = std::fs::read_to_string(dir.join("simucraft.toml")).unwrap();
    assert_eq!(check(Kind::Project, &manifest), []);

    // A second service opens what the first saved.
    let mut again = Service::default();
    ok(&mut again, "project.open", json!({"path": dir.to_str().unwrap()}));
    assert_eq!(
        ok(&mut again, "find", json!({"query": "core."}))
            .as_array()
            .unwrap()
            .len(),
        5
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bad_edits_are_refused_with_diagnostics() {
    let dir = scratch("bad");
    let mut svc = Service::default();
    ok(
        &mut svc,
        "project.create",
        json!({"path": dir.to_str().unwrap(), "name": "pid"}),
    );
    add(&mut svc, "core.Gain", "g", json!({"k": 2}));

    let err = call(
        &mut svc,
        "block.add",
        json!({"model": "models/other.scmodel", "type": "core.Gain", "name": "x"}),
    );
    assert_eq!(err.unwrap_err()["diagnostics"][0]["code"], "A001", "unknown model file");
    let err = call(
        &mut svc,
        "block.add",
        json!({"model": "models/pid.scmodel", "type": "core.Gain", "name": "g"}),
    )
    .unwrap_err();
    assert_eq!(err["diagnostics"][0]["code"], "F007", "duplicate name: {err:#}");
    let err = call(
        &mut svc,
        "connect",
        json!({"source": "nothing.y", "destination": "g.u"}),
    )
    .unwrap_err();
    assert_eq!(err["diagnostics"][0]["code"], "A001");
    let err = call(&mut svc, "tx.commit", json!({})).unwrap_err();
    assert_eq!(err["diagnostics"][0]["code"], "A002");
    let err = call(&mut svc, "net.connect", json!({"ports": ["g.y"]})).unwrap_err();
    assert_eq!(err["code"], -32601);

    // Removing a block also removes the inputs that read from it.
    add(&mut svc, "core.Gain", "h", json!({}));
    ok(&mut svc, "connect", json!({"source": "g.y", "destination": "h.u"}));
    let removed = ok(&mut svc, "block.remove", json!({"target": "g"}));
    assert_eq!(removed["diff"].as_array().unwrap().len(), 2, "{removed:#}");
    std::fs::remove_dir_all(dir).unwrap();
}
