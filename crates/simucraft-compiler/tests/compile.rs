//! WP1.1 done-when: the PID example and the signal-flow part of
//! `examples/motor_demo` compile, and every error names a block and a port.

use std::path::{Path, PathBuf};

use simucraft_compiler::compile;
use simucraft_contracts::block::{
    Block, BlockSpec, BlockType, Built, Ctx, Layout, ParamSpec, Params, PortSpec, SampleTime,
};
use simucraft_contracts::fake;
use simucraft_contracts::ir::{CompiledModel, ParamValue};
use simucraft_contracts::{Diagnostic, Severity};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn motor_demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/motor_demo/models/speed_control.scmodel")
}

fn disk(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Stand-ins for the physical blocks of the motor demo. The real ones come
/// with WP2.8; the compiler needs only their signal ports and parameters.
struct Stub(BlockSpec);

impl BlockType for Stub {
    fn spec(&self) -> &BlockSpec {
        &self.0
    }
    fn build(&self, _: &Params, widths: &[usize]) -> Result<Built, Diagnostic> {
        let layout = Layout {
            inputs: widths.to_vec(),
            outputs: vec![1; self.0.outputs.len()],
            feedthrough: vec![false; widths.len()],
            continuous_states: 0,
            discrete_states: 0,
            algebraic: 0,
            zero_crossings: 0,
            sample_time: SampleTime::Continuous,
        };
        Ok((Box::new(Silent), layout))
    }
}

struct Silent;
impl Block for Silent {
    fn outputs(&mut self, _: &mut Ctx) {}
}

fn stub(type_name: &str, inputs: &[&str], outputs: &[&str], params: &[&str]) -> Box<dyn BlockType> {
    let port = |n: &&str| PortSpec {
        name: n.to_string(),
        doc: String::new(),
        dtype: Default::default(),
        width: None,
        unit: None,
    };
    Box::new(Stub(BlockSpec {
        type_name: type_name.into(),
        doc: String::new(),
        params: params
            .iter()
            .map(|p| ParamSpec {
                name: p.to_string(),
                doc: String::new(),
                default: Some("1".into()),
                unit: None,
                tunable: false,
            })
            .collect(),
        inputs: inputs.iter().map(port).collect(),
        outputs: outputs.iter().map(port).collect(),
        connectors: vec![],
    }))
}

fn library() -> Vec<Box<dyn BlockType>> {
    let mut lib = simucraft_blocks::library();
    lib.extend([
        stub("elec.ControlledVoltageSource", &["v"], &[], &[]),
        stub("elec.Ground", &[], &[], &[]),
        stub("elec.Resistor", &[], &[], &["R"]),
        stub("elec.Inductor", &[], &[], &["L"]),
        stub("elec.RotationalEMF", &[], &[], &["k"]),
        stub("rot.SpeedSensor", &[], &["y"], &[]),
        stub("rot.Inertia", &[], &[], &["J"]),
        stub("rot.ViscousFriction", &[], &[], &["d"]),
    ]);
    lib
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter().filter(|d| d.is_error()).collect()
}

fn ok(path: &Path, load: &dyn Fn(&Path) -> Option<String>) -> (CompiledModel, Vec<Diagnostic>) {
    let (model, diagnostics) = compile(path, &library(), load);
    assert!(errors(&diagnostics).is_empty(), "{diagnostics:#?}");
    (model.expect("a model when there are no errors"), diagnostics)
}

fn block<'a>(m: &'a CompiledModel, id: &str) -> (usize, &'a simucraft_contracts::ir::BlockInstance) {
    m.blocks
        .iter()
        .enumerate()
        .find(|(_, b)| b.id == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

fn scalar(v: &ParamValue) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v:?}"))
}

#[test]
fn pid_example_compiles_and_runs() {
    let (m, diagnostics) = ok(&fixture("pid_loop.scmodel"), &disk);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let (_, pid) = block(&m, "b_000000000003");
    assert_eq!(scalar(&pid.params["P"]), 2.0);
    assert_eq!(scalar(&pid.params["I"]), 3.0);
    let (_, plant) = block(&m, "b_000000000004");
    assert_eq!(plant.params["den"], ParamValue::Vector(vec![0.5, 1.0]));
    assert_eq!(m.outports["y"], plant.outputs[0]);

    // Closed loop: PI on 1 / (0.5 s + 1) settles at the reference.
    let traces = fake::run_euler(&m, &library(), 1e-3, 10.0).unwrap();
    let (e, y) = (traces["e"].last().unwrap(), traces["plant_out"].last().unwrap());
    assert!(e.abs() < 1e-3 && (y - 1.0).abs() < 1e-3, "e = {e}, y = {y}");
}

#[test]
fn motor_demo_signal_flow_part_compiles() {
    let (m, diagnostics) = ok(&motor_demo(), &disk);
    // The nets of both files wait for the acausal compiler (WP2.7).
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["C009", "C009"], "{diagnostics:#?}");
    assert!(diagnostics.iter().all(|d| d.severity == Severity::Warning));

    let pos = |id: &str| block(&m, id).0;
    let (speed_ref, sensor, error, pi) = (
        pos("b_1c2d4r8vn3ex"),
        pos("b_e5f20hp8k1wt"),
        pos("b_4a10f3k8m2qx"),
        pos("b_5c2d9tz6hyaw"),
    );
    assert!(speed_ref < error && sensor < error && error < pi);

    let (_, pi) = block(&m, "b_5c2d9tz6hyaw");
    assert_eq!(
        (
            scalar(&pi.params["P"]),
            scalar(&pi.params["I"]),
            scalar(&pi.params["ts"])
        ),
        (0.8, 4.0, 0.001)
    );
    assert_eq!(
        m.rates[pi.rate],
        SampleTime::Discrete {
            period: 0.001,
            offset: 0.0
        }
    );
    let (_, error) = block(&m, "b_4a10f3k8m2qx");
    assert_eq!(m.rates[error.rate], SampleTime::Continuous);

    // Flattened subsystem: IDs are paths, parameters come from the parent.
    let motor = |id: &str| block(&m, &format!("b_9b18ejx4s9mk/{id}")).1;
    assert_eq!(scalar(&motor("b_mm9rdp3pcrzt").params["R"]), 1.2);
    assert!((scalar(&motor("b_k4gxahnan2ed").params["J"]) - 0.0121).abs() < 1e-15);
    assert!((scalar(&motor("b_6t0eqt79m4nx").params["d"]) - 1e-4).abs() < 1e-18);

    let (_, sensor) = block(&m, "b_e5f20hp8k1wt");
    assert_eq!(m.outports["speed"], sensor.outputs[0]);
    assert_eq!(m.inports["speed_ref"], block(&m, "b_1c2d4r8vn3ex").1.outputs[0]);
    assert_eq!(m.logged["speed_error"], error.outputs[0]);
}

/// Compile the PID fixture with one text edit and return its errors.
fn broken(from: &str, to: &str) -> Vec<Diagnostic> {
    let path = fixture("pid_loop.scmodel");
    let text = disk(&path).unwrap();
    assert!(text.contains(from), "fixture has no `{from}`");
    let edited = text.replace(from, to);
    let (model, diagnostics) = compile(&path, &library(), &|_| Some(edited.clone()));
    assert!(model.is_none());
    diagnostics.into_iter().filter(Diagnostic::is_error).collect()
}

#[test]
fn every_error_names_a_block_and_a_port() {
    let cases = [
        // (edit, code, block ID, path)
        (
            ("u2 = \"b_000000000004.y\"", "u2 = \"b_000000000004.q\""),
            "C002",
            "b_000000000002",
            "error.u2",
        ),
        (
            ("u2 = \"b_000000000004.y\"\n", ""),
            "C003",
            "b_000000000002",
            "error.u2",
        ),
        (("D = 0.0", "D = 0.0\nKd = 1.0"), "C006", "b_000000000003", "pid.Kd"),
        (("P = \"Kp\"", "P = \"Kp *\""), "C101", "b_000000000003", "pid.P"),
        (("P = \"Kp\"", "P = \"Kq\""), "C102", "b_000000000003", "pid.P"),
        (
            ("type = \"core.Constant\"", "type = \"core.Nothing\""),
            "C001",
            "b_000000000001",
            "ref",
        ),
        // s / (0.5 s + 1) has direct feedthrough, so the loop is algebraic.
        (
            ("num = [1.0]", "num = [1.0, 0.0]"),
            "C005",
            "b_000000000002",
            "error.u2",
        ),
    ];
    for ((from, to), code, id, path) in cases {
        let errs = broken(from, to);
        let found = errs
            .iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| panic!("{code}: {errs:#?}"));
        assert_eq!(
            (found.id.as_deref(), found.path.as_deref()),
            (Some(id), Some(path)),
            "{found:#?}"
        );
        assert!(
            found.file.as_deref().is_some_and(|f| f.ends_with("pid_loop.scmodel")),
            "{found:#?}"
        );
    }
    let algebraic = broken("num = [1.0]", "num = [1.0, 0.0]");
    let msg = &algebraic.iter().find(|d| d.code == "C005").unwrap().message;
    assert!(
        msg.contains("error") && msg.contains("pid") && msg.contains("plant"),
        "{msg}"
    );
}

#[test]
fn a_missing_subsystem_file_names_the_subsystem_block() {
    let path = motor_demo();
    let (model, diagnostics) = compile(&path, &library(), &|p| {
        if p.ends_with("dc_motor.scmodel") { None } else { disk(p) }
    });
    assert!(model.is_none());
    let d = errors(&diagnostics)[0];
    assert_eq!(
        (d.code.as_str(), d.id.as_deref(), d.path.as_deref()),
        ("C008", Some("b_9b18ejx4s9mk"), Some("motor"))
    );
}

#[test]
fn widths_propagate_through_a_feedback_loop() {
    // u (width 2) -> sum -> integrator -> back into sum
    let text = r#"format = "simucraft.model/0.1"
id = "m_00000000000w"
name = "widths"

[blocks.b_000000000001]
name = "u"
type = "core.Inport"

[blocks.b_000000000001.params]
width = 2

[blocks.b_000000000002]
name = "sum"
type = "core.Sum"

[blocks.b_000000000002.params]
signs = "+-"

[blocks.b_000000000002.inputs]
u1 = "b_000000000001.y"
u2 = "b_000000000003.y"

[blocks.b_000000000003]
name = "int"
type = "core.Integrator"

[blocks.b_000000000003.inputs]
u = "b_000000000002.y"
"#;
    let (m, _) = ok(Path::new("widths.scmodel"), &|_| Some(text.to_string()));
    assert!(
        m.blocks.iter().all(|b| b.outputs.iter().all(|s| s.len == 2)),
        "{:#?}",
        m.blocks
    );
    assert_eq!(m.continuous_states, 2);
}
