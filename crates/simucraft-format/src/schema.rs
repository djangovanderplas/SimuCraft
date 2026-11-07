//! Typed schemas for the six file kinds. Unknown keys are errors.
//!
//! Maps are `BTreeMap`, so iteration is in key order. These types are for
//! reading and checking; edits go through [`crate::File`] so comments survive.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Any value a parameter or override can hold.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Array(Vec<Value>),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Simulation {
    pub solver: Option<String>,
    pub start_time: Option<f64>,
    pub stop_time: Option<f64>,
    pub rel_tol: Option<f64>,
    pub abs_tol: Option<f64>,
    pub max_step: Option<f64>,
    pub fixed_step: Option<f64>,
}

// ---------------------------------------------------------------- project

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub format: String,
    pub name: String,
    pub top: String,
    #[serde(default)]
    pub libraries: BTreeMap<String, String>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub units: Units,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default)]
    pub simulation: Simulation,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Units {
    /// Warn when an unspecified unit meets a declared one (spec Section 9).
    #[serde(default)]
    pub strict: bool,
}

// ---------------------------------------------------------------- lock

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub format: String,
    #[serde(default)]
    pub libraries: BTreeMap<String, Locked>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locked {
    pub version: String,
    pub hash: String,
}

// ---------------------------------------------------------------- model

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub format: String,
    pub id: String,
    pub name: String,
    pub doc: Option<String>,
    pub simulation: Option<Simulation>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Parameter>,
    #[serde(default)]
    pub blocks: BTreeMap<String, Block>,
    #[serde(default)]
    pub nets: BTreeMap<String, Net>,
    #[serde(default)]
    pub signals: BTreeMap<String, Signal>,
    pub mask: Option<Mask>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub value: Option<Value>,
    pub expr: Option<String>,
    pub file: Option<String>,
    pub key: Option<String>,
    pub unit: Option<String>,
    pub doc: Option<String>,
    #[serde(default)]
    pub tunable: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub uncertain: Option<Distribution>,
}

/// A probability distribution: `dist` plus the fields that dist needs.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Distribution {
    pub dist: Option<Dist>,
    pub std: Option<f64>,
    pub mean: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    /// Relative spread around the nominal value.
    pub rel: Option<f64>,
    pub values: Option<Vec<Value>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dist {
    Normal,
    Uniform,
    Lognormal,
    Choice,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Subsystem file, for `type = "subsystem"`.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    /// Sample time; omitted means inherited.
    pub ts: Option<f64>,
    pub doc: Option<String>,
    #[serde(default)]
    pub trace: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    /// Input port name -> source port (`<block id>.<port>`).
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Net {
    pub ports: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signal {
    pub name: Option<String>,
    #[serde(default)]
    pub log: bool,
    #[serde(default)]
    pub analysis_point: bool,
    #[serde(default)]
    pub trace: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mask {
    pub icon: Option<String>,
    /// Optional Python hook `file.py:function`; runs only in trusted projects.
    pub hook: Option<String>,
    #[serde(default)]
    pub prompts: BTreeMap<String, Prompt>,
    /// Derived parameter name -> expression.
    #[serde(default)]
    pub derived: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    pub label: String,
    pub doc: Option<String>,
}

// ---------------------------------------------------------------- layout

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub format: String,
    pub model: String,
    #[serde(default)]
    pub blocks: BTreeMap<String, Placement>,
    /// Signal wires, keyed by destination port.
    #[serde(default)]
    pub wires: BTreeMap<String, Route>,
    #[serde(default)]
    pub nets: BTreeMap<String, Route>,
    #[serde(default)]
    pub annotations: BTreeMap<String, Annotation>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub at: [f64; 2],
    pub size: Option<[f64; 2]>,
    pub orient: Option<Orient>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orient {
    Right,
    Left,
    Up,
    Down,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub text: String,
    pub at: [f64; 2],
}

// ---------------------------------------------------------------- experiment

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    pub format: String,
    pub name: String,
    pub model: String,
    pub seed: Option<u64>,
    pub runs: Option<u64>,
    pub sampling: Option<Sampling>,
    pub simulation: Option<Simulation>,
    #[serde(default)]
    pub inputs: BTreeMap<String, Input>,
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    #[serde(default)]
    pub vary: BTreeMap<String, Vary>,
    pub record: Option<Record>,
    #[serde(default)]
    pub metrics: BTreeMap<String, String>,
    pub requirements: Option<Requirements>,
    pub execution: Option<Execution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sampling {
    Grid,
    MonteCarlo,
    Lhs,
    Sobol,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub signal: InputSignal,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    /// Data file for `signal = "table"`.
    pub file: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSignal {
    Constant,
    Step,
    Ramp,
    Sine,
    Table,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vary {
    /// Reuse the parameter's own `uncertain` entry.
    #[serde(default)]
    pub from_model: bool,
    /// Pins the random stream key (spec Section 6).
    pub stream: Option<String>,
    pub dist: Option<Dist>,
    pub std: Option<f64>,
    pub mean: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub rel: Option<f64>,
    /// Grid points, or the options for `dist = "choice"`.
    pub values: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    #[serde(default)]
    pub signals: Vec<String>,
    /// all | none | failures | sample:<n>
    pub traces: Option<String>,
    pub decimate: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct Requirements {
    /// Named requirements imported from `.scdesign` files (`file#name`).
    #[serde(default)]
    pub uses: Vec<String>,
    /// Requirement name -> boolean expression over metrics.
    #[serde(flatten)]
    pub checks: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub executor: Option<String>,
    pub workers: Option<Workers>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Workers {
    Count(u32),
    Auto(String),
}

// ---------------------------------------------------------------- design

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub format: String,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub operating_points: BTreeMap<String, OperatingPoint>,
    #[serde(default)]
    pub tunable: BTreeMap<String, Tunable>,
    #[serde(default)]
    pub loops: BTreeMap<String, Loop>,
    #[serde(default)]
    pub requirements: BTreeMap<String, Requirement>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatingPoint {
    pub method: Option<OpMethod>,
    pub base: Option<String>,
    #[serde(default)]
    pub free: Vec<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, f64>,
    #[serde(default)]
    pub grid: BTreeMap<String, Vec<f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpMethod {
    Trim,
    Snapshot,
    Initial,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tunable {
    pub block: String,
    pub form: TunableForm,
    #[serde(default)]
    pub params: BTreeMap<String, Bounds>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunableForm {
    Scalar,
    Pid,
    Tf,
    Ss,
    GainSurface,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub fixed: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Loop {
    pub open_at: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub kind: RequirementKind,
    #[serde(default)]
    pub hard: bool,
    /// Operating point(s) it applies to.
    pub at: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "loop")]
    pub loop_name: Option<String>,
    pub time_constant: Option<f64>,
    pub settling_time: Option<f64>,
    pub overshoot: Option<f64>,
    pub gain_db: Option<f64>,
    pub phase_deg: Option<f64>,
    pub max_peak_db: Option<f64>,
    pub band_hz: Option<[f64; 2]>,
    pub min_damping: Option<f64>,
    pub min_decay: Option<f64>,
    pub max_freq_hz: Option<f64>,
    pub max: Option<f64>,
    pub expr: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    StepTracking,
    Margins,
    Sensitivity,
    MaxGain,
    MinGain,
    Poles,
    Variance,
    Assert,
}
