//! The SimuCraft MCP server (spec Section 8, WP1.6). It speaks the Model
//! Context Protocol: JSON-RPC 2.0, one message per line. It gives agents six
//! task-level tools on one project, and serves the model files as resources.
//!
//! The files on disk are the truth. `edit` saves after each call, and every
//! tool opens the project again first, so an agent can also edit the files
//! directly and then call `check`. The server never trusts a project (spec
//! Section 10): no tool can do that.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command as Process;

use serde_json::{Value as Json, json};
use simucraft_contracts::api::{Outcome, Request, RpcError};
use simucraft_format::Kind;
use simucraft_service::Service;

/// Protocol versions this server knows, newest first.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The Model API commands that `edit` accepts.
const EDITS: &[&str] = &[
    "block.add",
    "block.remove",
    "block.rename",
    "block.set_params",
    "connect",
    "disconnect",
    "param.set",
    "param.remove",
    "signal.set",
];

/// Points per signal that `signal_summary` gives when the agent does not say.
const SUMMARY_POINTS: usize = 50;

const RESOURCE_PREFIX: &str = "simucraft://model/";

/// One MCP session on one project.
pub struct Server {
    service: Service,
    root: PathBuf,
    /// Run ID -> its signal names, for `signal_summary` without a list.
    runs: BTreeMap<String, Vec<String>>,
}

/// A failed tool call: a message and the structured diagnostics.
struct ToolError(Json);

impl From<RpcError> for ToolError {
    fn from(e: RpcError) -> Self {
        ToolError(json!({"message": e.message, "diagnostics": e.diagnostics}))
    }
}

fn tool_error(message: impl Into<String>) -> ToolError {
    ToolError(json!({"message": message.into(), "diagnostics": []}))
}

impl Server {
    /// Open the project in folder `root`.
    pub fn open(root: &Path) -> Result<Self, String> {
        let mut server = Server {
            service: Service::default(),
            root: root.to_path_buf(),
            runs: BTreeMap::new(),
        };
        server
            .reload()
            .map_err(|e| e.0["message"].as_str().unwrap_or_default().to_string())?;
        Ok(server)
    }

    /// Answer one JSON-RPC message. Notifications get no answer.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let msg: Json = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(e) => return Some(reply(&Json::Null, Err((-32700, format!("not JSON: {e}"))))),
        };
        let id = msg.get("id")?.clone();
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => {
                let asked = params["protocolVersion"].as_str().unwrap_or_default();
                let version = VERSIONS.iter().find(|v| **v == asked).unwrap_or(&VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {}, "resources": {}},
                    "serverInfo": {"name": "simucraft", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Tools work on the files on disk. You can also edit the project files \
                                     directly and then call `check`. Diagnostics name a block and a port.",
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or_default().to_string();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Ok(match self.tool(&name, &args) {
                    Ok(v) => json!({"content": [text(&v)], "isError": false}),
                    Err(ToolError(e)) => json!({"content": [text(&e)], "isError": true}),
                })
            }
            "resources/list" => self.resources(),
            "resources/read" => self.read_resource(params["uri"].as_str().unwrap_or_default()),
            other => Err((-32601, format!("unknown method `{other}`"))),
        };
        Some(reply(&id, result))
    }

    fn tool(&mut self, name: &str, args: &Json) -> Result<Json, ToolError> {
        match name {
            "check" => self.check(args),
            "edit" => self.edit(args),
            "simulate" => self.simulate(args),
            "signal_summary" => self.signal_summary(args),
            "diff" => self.diff(args),
            "library_docs" => match args["type"].as_str() {
                Some(t) => self.call("library.describe", json!({"type_name": t})),
                None => self.call("library.list", json!({"filter": args["query"]})),
            },
            other => Err(tool_error(format!("unknown tool `{other}`"))),
        }
    }

    /// Send one Model API command to the service.
    fn call(&mut self, method: &str, params: Json) -> Result<Json, ToolError> {
        let request: Request =
            serde_json::from_value(json!({"jsonrpc": "2.0", "id": 0, "method": method, "params": params}))
                .map_err(|e| tool_error(format!("`{method}`: {e}")))?;
        match self.service.handle(request).outcome {
            Outcome::Result(v) => Ok(v),
            Outcome::Error(e) => Err(e.into()),
        }
    }

    /// Read the project from disk again. Returns the diagnostics of files that do not open.
    fn reload(&mut self) -> Result<Json, ToolError> {
        let root = self.root.to_string_lossy().to_string();
        self.call("project.open", json!({"path": root}))
    }

    /// Compile a model, or check any other project file.
    fn check(&mut self, args: &Json) -> Result<Json, ToolError> {
        let opened = self.reload()?;
        let path = match args["path"].as_str() {
            Some(p) => inside(p)?,
            None => opened["top"].as_str().unwrap_or_default().to_string(),
        };
        let mut diagnostics: Vec<Json> = opened["diagnostics"].as_array().cloned().unwrap_or_default();
        let open_models = opened["models"].as_array().cloned().unwrap_or_default();
        match Kind::from_path(Path::new(&path)) {
            Some(Kind::Model) if open_models.contains(&json!(path)) => {
                let compiled = self.call("compile", json!({"model": path}))?;
                diagnostics.extend(compiled["diagnostics"].as_array().cloned().unwrap_or_default());
            }
            // A model file that does not parse: its diagnostics came from `project.open`.
            Some(Kind::Model) => {}
            Some(kind) => {
                let text =
                    std::fs::read_to_string(self.root.join(&path)).map_err(|e| tool_error(format!("`{path}`: {e}")))?;
                for mut d in simucraft_format::check(kind, &text) {
                    d.file = Some(path.clone());
                    diagnostics.push(json!(d));
                }
            }
            None => return Err(tool_error(format!("`{path}` is not a SimuCraft project file"))),
        }
        let ok = !diagnostics.iter().any(|d| d["severity"] == "error");
        Ok(json!({"path": path, "ok": ok, "diagnostics": diagnostics}))
    }

    /// Run edit commands as one transaction and save. With `dry_run`, roll back and keep the diffs.
    fn edit(&mut self, args: &Json) -> Result<Json, ToolError> {
        let commands = args["commands"]
            .as_array()
            .ok_or_else(|| tool_error("`commands` must be a list of {method, params}"))?;
        let dry_run = args["dry_run"].as_bool().unwrap_or(false);
        self.reload()?;
        self.call("tx.begin", json!({"label": "mcp edit"}))?;
        let mut results = Vec::new();
        for (i, c) in commands.iter().enumerate() {
            let method = c["method"].as_str().unwrap_or_default();
            let outcome = if EDITS.contains(&method) {
                self.call(method, c.get("params").cloned().unwrap_or(json!({})))
            } else {
                Err(tool_error(format!(
                    "`{method}` is not an edit command; use one of {}",
                    EDITS.join(", ")
                )))
            };
            match outcome {
                Ok(r) => results.push(r),
                Err(ToolError(mut e)) => {
                    self.call("tx.rollback", json!({}))?;
                    e["failed_command"] = json!(i);
                    e["message"] = json!(format!(
                        "command {i} (`{method}`): {}",
                        e["message"].as_str().unwrap_or("")
                    ));
                    return Err(ToolError(e));
                }
            }
        }
        if dry_run {
            self.call("tx.rollback", json!({}))?;
            return Ok(json!({"dry_run": true, "results": results}));
        }
        self.call("tx.commit", json!({}))?;
        let saved = self.call("project.save", json!({}))?;
        Ok(json!({"results": results, "saved": saved["written"]}))
    }

    fn simulate(&mut self, args: &Json) -> Result<Json, ToolError> {
        self.reload()?;
        let model = args["model"].as_str().map(inside).transpose()?.unwrap_or_default();
        let run = self.call(
            "simulate",
            json!({"model": model, "stop_time": args["stop_time"], "signals": args.get("signals").cloned().unwrap_or(json!([]))}),
        )?;
        let signals = run["signals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .map(String::from);
        self.runs
            .insert(run["run"].as_str().unwrap_or_default().into(), signals.collect());
        Ok(run)
    }

    /// Statistics and a short trace of each signal of a run.
    fn signal_summary(&mut self, args: &Json) -> Result<Json, ToolError> {
        let run = args["run"]
            .as_str()
            .ok_or_else(|| tool_error("`run` is required"))?
            .to_string();
        let names: Vec<String> = match args["signals"].as_array() {
            Some(list) => list.iter().filter_map(Json::as_str).map(String::from).collect(),
            None => self.runs.get(&run).cloned().unwrap_or_default(),
        };
        let points = args["points"].as_u64().map_or(SUMMARY_POINTS, |p| p.max(2) as usize);
        let mut out = serde_json::Map::new();
        for name in names {
            let mut summary = self.call("signal.get", json!({"run": run, "signal": name, "mode": "summary"}))?;
            let trace = self.call("signal.get", json!({"run": run, "signal": name, "mode": "decimated"}))?;
            let (t, v, w) = (
                trace["t"].as_array().cloned().unwrap_or_default(),
                &trace["values"],
                trace["width"].as_u64().unwrap_or(1) as usize,
            );
            let stride = t.len().div_ceil(points).max(1);
            let mut rows: Vec<usize> = (0..t.len()).step_by(stride).collect();
            if !t.is_empty() && rows.last() != Some(&(t.len() - 1)) {
                rows.push(t.len() - 1);
            }
            summary["trace"] = json!({
                "t": rows.iter().map(|&r| &t[r]).collect::<Vec<_>>(),
                "values": rows.iter().flat_map(|&r| (0..w).map(move |k| &v[r * w + k])).collect::<Vec<_>>(),
            });
            out.insert(name, summary);
        }
        Ok(Json::Object(out))
    }

    /// Semantic diff of a file: against a Git ref (default `HEAD`), or against a second file.
    fn diff(&mut self, args: &Json) -> Result<Json, ToolError> {
        let path = inside(args["path"].as_str().ok_or_else(|| tool_error("`path` is required"))?)?;
        let read = |p: &str| std::fs::read_to_string(self.root.join(p)).map_err(|e| tool_error(format!("`{p}`: {e}")));
        let (old, new) = match args["other"].as_str() {
            Some(other) => (read(&path)?, read(&inside(other)?)?),
            None => {
                let rev = args["ref"].as_str().unwrap_or("HEAD");
                if rev.starts_with('-') || rev.contains(':') {
                    return Err(tool_error(format!("`{rev}` is not a Git ref")));
                }
                (self.git_show(rev, &path)?, read(&path)?)
            }
        };
        let changes = simucraft_service::semantic_diff(&path, &old, &new).map_err(tool_error)?;
        Ok(json!({"path": path, "changes": changes}))
    }

    /// A file as it is at Git ref `rev`.
    fn git_show(&self, rev: &str, path: &str) -> Result<String, ToolError> {
        // fsmonitor and textconv can run programs that a repository names; turn both off.
        let out = Process::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["-c", "core.fsmonitor=false", "--no-pager", "show", "--no-textconv"])
            .arg(format!("{rev}:./{path}"))
            .output()
            .map_err(|e| tool_error(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            return Err(tool_error(String::from_utf8_lossy(&out.stderr).trim().to_string()));
        }
        String::from_utf8(out.stdout).map_err(|_| tool_error(format!("`{path}` at `{rev}` is not UTF-8")))
    }

    fn resources(&mut self) -> Result<Json, (i64, String)> {
        let opened = self.reload().map_err(protocol)?;
        let list: Vec<Json> = opened["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .map(|m| json!({"uri": format!("{RESOURCE_PREFIX}{m}"), "name": m, "mimeType": "application/json"}))
            .collect();
        Ok(json!({"resources": list}))
    }

    fn read_resource(&mut self, uri: &str) -> Result<Json, (i64, String)> {
        let path = uri
            .strip_prefix(RESOURCE_PREFIX)
            .ok_or((-32602, format!("unknown resource `{uri}`")))?;
        self.reload().map_err(protocol)?;
        let model = self.call("model.get", json!({"path": path})).map_err(protocol)?;
        Ok(json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": model.to_string()}]}))
    }
}

/// A project path from an agent: relative, and it stays inside the project.
fn inside(path: &str) -> Result<String, ToolError> {
    let p = Path::new(path);
    if p.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && !path.is_empty()
    {
        Ok(path.trim_start_matches("./").replace('\\', "/"))
    } else {
        Err(tool_error(format!(
            "`{path}` must be a path inside the project, relative to its root"
        )))
    }
}

fn protocol(e: ToolError) -> (i64, String) {
    (-32602, e.0["message"].as_str().unwrap_or_default().to_string())
}

fn text(v: &Json) -> Json {
    json!({"type": "text", "text": serde_json::to_string_pretty(v).expect("JSON serializes")})
}

fn reply(id: &Json, result: Result<Json, (i64, String)>) -> String {
    let msg = match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
    };
    msg.to_string()
}

/// The six tools of spec Section 8, with their argument schemas.
fn tools() -> Json {
    let path =
        json!({"type": "string", "description": "File path from the project root, for example models/pid.scmodel"});
    json!([
        {
            "name": "check",
            "description": "Compile a model, or check any other project file. Returns structured diagnostics; each names a block and a port or parameter. Reads the files on disk, so call it after you edit a file directly.",
            "inputSchema": {"type": "object", "properties": {"path": path}},
        },
        {
            "name": "edit",
            "description": format!("Apply Model API edit commands as one transaction, then save. Each command is {{method, params}}; the methods are {}. Ports are `<block name>.<port>`, for example `pid.y`. With dry_run, nothing changes and the diffs come back.", EDITS.join(", ")),
            "inputSchema": {"type": "object", "required": ["commands"], "properties": {
                "commands": {"type": "array", "items": {"type": "object", "required": ["method"], "properties": {
                    "method": {"type": "string", "enum": EDITS},
                    "params": {"type": "object"},
                }}},
                "dry_run": {"type": "boolean"},
            }},
        },
        {
            "name": "simulate",
            "description": "Compile and run a model and wait for the result. The solver settings come from the model's [simulation] table. Returns a run ID for signal_summary.",
            "inputSchema": {"type": "object", "properties": {
                "model": path,
                "stop_time": {"type": "number"},
                "signals": {"type": "array", "items": {"type": "string"}, "description": "Outports or logged signals to record; the default is all"},
            }},
        },
        {
            "name": "signal_summary",
            "description": "Statistics (min, max, mean, final value, with their times) and a short trace for signals of a run.",
            "inputSchema": {"type": "object", "required": ["run"], "properties": {
                "run": {"type": "string"},
                "signals": {"type": "array", "items": {"type": "string"}},
                "points": {"type": "integer", "description": format!("Most points in each trace; the default is {SUMMARY_POINTS}")},
            }},
        },
        {
            "name": "diff",
            "description": "Semantic diff of a project file: against a Git ref (default HEAD), or against a second file.",
            "inputSchema": {"type": "object", "required": ["path"], "properties": {
                "path": path,
                "ref": {"type": "string"},
                "other": {"type": "string"},
            }},
        },
        {
            "name": "library_docs",
            "description": "Search block types (query), or read one type's ports, parameters, defaults, units and docs (type).",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string"},
                "type": {"type": "string", "description": "A block type, for example core.PID"},
            }},
        },
    ])
}
