//! Model API: JSON-RPC 2.0 commands shared by the editor, Python and MCP
//! (spec Section 8). Requests carry `{"method": ..., "params": {...}}`.
//!
//! Edit commands take `dry_run` and return an [`EditResult`] with the
//! semantic diff. Targets (`block`, `port`, `model`) accept a name path
//! (`guidance/pid`, `pid.y`) or a stable ID (`b_5c2d9tz6hyaw.y`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::Diagnostic;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// Absent for notifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Json>,
    #[serde(flatten)]
    pub command: Command,
}

impl Request {
    pub fn new(id: u64, command: Command) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id.into()),
            command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<Json>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Json),
    Error(RpcError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// JSON-RPC code: -32601 unknown method, -32602 bad params, 1 command failed.
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

/// An edit command's arguments plus `dry_run`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edit<T> {
    #[serde(flatten)]
    pub args: T,
    /// Return the diff without applying it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
}

type Map = BTreeMap<String, Json>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Command {
    // project
    #[serde(rename = "project.create")]
    ProjectCreate { path: String, name: String },
    #[serde(rename = "project.open")]
    ProjectOpen { path: String },
    #[serde(rename = "project.save")]
    ProjectSave {},
    #[serde(rename = "project.status")]
    ProjectStatus {},
    /// Editor and CLI only; the MCP server refuses it.
    #[serde(rename = "project.trust")]
    ProjectTrust { path: String, trusted: bool },

    // blocks
    #[serde(rename = "block.add")]
    BlockAdd(Edit<BlockAdd>),
    #[serde(rename = "block.remove")]
    BlockRemove(Edit<Target>),
    #[serde(rename = "block.rename")]
    BlockRename(Edit<Rename>),
    #[serde(rename = "block.set_params")]
    BlockSetParams(Edit<SetParams>),

    // wiring
    /// Sets the one source of a signal input, replacing any previous source.
    #[serde(rename = "connect")]
    Connect(Edit<Connect>),
    #[serde(rename = "disconnect")]
    Disconnect(Edit<Destination>),
    #[serde(rename = "net.connect")]
    NetConnect(Edit<Ports>),
    #[serde(rename = "net.disconnect")]
    NetDisconnect(Edit<Ports>),

    // interface: model ports are core.Inport, core.Outport and core.Connector blocks
    #[serde(rename = "port.add")]
    PortAdd(Edit<PortAdd>),
    #[serde(rename = "port.remove")]
    PortRemove(Edit<Target>),

    // parameters and signals
    #[serde(rename = "param.set")]
    ParamSet(Edit<ParamSet>),
    #[serde(rename = "param.remove")]
    ParamRemove(Edit<ParamName>),
    #[serde(rename = "signal.set")]
    SignalSet(Edit<SignalSet>),

    // structure and layout
    #[serde(rename = "subsystem.create")]
    SubsystemCreate(Edit<SubsystemCreate>),
    #[serde(rename = "subsystem.expand")]
    SubsystemExpand(Edit<Target>),
    #[serde(rename = "layout.set")]
    LayoutSet(Edit<LayoutSet>),
    #[serde(rename = "annotation.add")]
    AnnotationAdd(Edit<AnnotationAdd>),

    // session
    #[serde(rename = "tx.begin")]
    TxBegin { label: String },
    #[serde(rename = "tx.commit")]
    TxCommit {},
    #[serde(rename = "tx.rollback")]
    TxRollback {},
    #[serde(rename = "undo")]
    Undo {},
    #[serde(rename = "redo")]
    Redo {},

    // query
    #[serde(rename = "model.get")]
    ModelGet { path: String, depth: Option<u32> },
    #[serde(rename = "find")]
    Find { query: String },
    #[serde(rename = "library.list")]
    LibraryList { filter: Option<String> },
    #[serde(rename = "library.describe")]
    LibraryDescribe { type_name: String },

    // run
    #[serde(rename = "compile")]
    Compile { model: String },
    #[serde(rename = "simulate")]
    Simulate(Simulate),
    #[serde(rename = "run.stop")]
    RunStop { run: String },
    #[serde(rename = "signal.get")]
    SignalGet {
        run: String,
        signal: String,
        mode: SignalMode,
    },

    // experiments
    /// `file` is an .scexp path; `spec` an inline experiment as JSON.
    #[serde(rename = "experiment.run")]
    ExperimentRun { file: Option<String>, spec: Option<Json> },
    #[serde(rename = "experiment.status")]
    ExperimentStatus { experiment: String },
    #[serde(rename = "experiment.results")]
    ExperimentResults { experiment: String },
    #[serde(rename = "experiment.replay")]
    ExperimentReplay { experiment: String, run: u64 },

    // versions and design
    /// Refs: `worktree`, a file path, or a Git ref such as `HEAD~1`.
    #[serde(rename = "diff")]
    Diff { from: String, to: String },
    #[serde(rename = "merge")]
    Merge { base: String, ours: String, theirs: String },
    #[serde(rename = "design.validate")]
    DesignValidate { file: String },
}

/// Every method name, for coverage tests and for clients that list them.
pub const METHODS: &[&str] = &[
    "project.create",
    "project.open",
    "project.save",
    "project.status",
    "project.trust",
    "block.add",
    "block.remove",
    "block.rename",
    "block.set_params",
    "connect",
    "disconnect",
    "net.connect",
    "net.disconnect",
    "port.add",
    "port.remove",
    "param.set",
    "param.remove",
    "signal.set",
    "subsystem.create",
    "subsystem.expand",
    "layout.set",
    "annotation.add",
    "tx.begin",
    "tx.commit",
    "tx.rollback",
    "undo",
    "redo",
    "model.get",
    "find",
    "library.list",
    "library.describe",
    "compile",
    "simulate",
    "run.stop",
    "signal.get",
    "experiment.run",
    "experiment.status",
    "experiment.results",
    "experiment.replay",
    "diff",
    "merge",
    "design.validate",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockAdd {
    /// Model file or subsystem path the block goes into.
    pub model: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(default)]
    pub params: Map,
    /// Input port -> source port.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rename {
    pub target: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetParams {
    pub target: String,
    /// `null` removes a parameter.
    pub params: Map,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connect {
    pub source: String,
    pub destination: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Destination {
    pub destination: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ports {
    pub ports: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortKind {
    Inport,
    Outport,
    Connector,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortAdd {
    pub model: String,
    pub kind: PortKind,
    pub name: String,
    pub dtype: Option<String>,
    pub unit: Option<String>,
    /// For connectors: `elec`, `trans`, `rot`, `thermal`.
    pub domain: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamSet {
    pub model: String,
    pub name: String,
    /// Exactly one of `value`, `expr`, `file`.
    pub value: Option<Json>,
    pub expr: Option<String>,
    pub file: Option<String>,
    pub unit: Option<String>,
    pub tunable: Option<bool>,
    pub uncertain: Option<Json>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamName {
    pub model: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalSet {
    pub port: String,
    pub name: Option<String>,
    pub log: Option<bool>,
    pub analysis_point: Option<bool>,
    pub trace: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubsystemCreate {
    pub blocks: Vec<String>,
    pub name: String,
    /// Save the subsystem to its own file.
    pub file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutSet {
    pub target: String,
    pub at: Option<[f64; 2]>,
    pub size: Option<[f64; 2]>,
    pub orient: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnotationAdd {
    pub model: String,
    pub text: String,
    pub at: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Simulate {
    pub model: String,
    /// Parameter path -> value, for this run only.
    #[serde(default)]
    pub overrides: Map,
    pub stop_time: Option<f64>,
    /// Signals to record; empty means the model's logged signals.
    #[serde(default)]
    pub signals: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalMode {
    Summary,
    Decimated,
    Full,
}

// ---------------------------------------------------------------- results

/// What every edit command returns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditResult {
    pub diff: Vec<Change>,
    /// IDs created by the edit, in order.
    #[serde(default)]
    pub created: Vec<String>,
    /// Diagnostics the edit caused (the edit still applies).
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// One entry of a semantic diff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub op: ChangeOp,
    /// File the change is in, relative to the project root.
    pub file: String,
    /// Key path in that file: `blocks.b_…inputs.u1`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Json>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Add,
    Remove,
    Set,
}

/// Events pushed to subscribed clients: method = event name, params = payload.
pub const EVENTS: &[&str] = &[
    "model.changed",
    "files.reloaded",
    "sync.conflict",
    "diagnostics.changed",
    "run.progress",
    "run.finished",
];
