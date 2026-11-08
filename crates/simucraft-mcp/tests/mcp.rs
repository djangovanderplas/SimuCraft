//! WP1.6 done-when: an agent builds, runs and fixes the PID example, through
//! `edit` and by editing files then calling `check`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use simucraft_mcp::Server;

/// A new project folder with one empty model, `models/pid.scmodel`.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("simucraft-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut svc = simucraft_service::Service::default();
    let create = json!({"jsonrpc": "2.0", "id": 1, "method": "project.create",
                        "params": {"path": dir.to_str().unwrap(), "name": "pid"}});
    assert!(svc.handle_json(&create.to_string()).contains("\"result\""));
    dir
}

struct Agent {
    server: Server,
    id: u64,
}

impl Agent {
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let line = json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params}).to_string();
        let reply: Value = serde_json::from_str(&self.server.handle(&line).expect("a request gets a reply")).unwrap();
        assert_eq!(reply["id"], self.id);
        reply
    }

    /// Call a tool. Returns (is error, the tool's JSON result).
    fn tool(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": arguments}))["result"].clone();
        let text = r["content"][0]["text"].as_str().expect("text content");
        (r["isError"].as_bool().unwrap(), serde_json::from_str(text).unwrap())
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let (failed, v) = self.tool(name, arguments);
        assert!(!failed, "{name}: {v:#}");
        v
    }
}

fn cmd(method: &str, params: Value) -> Value {
    json!({"method": method, "params": params})
}

fn add(ty: &str, name: &str, params: Value) -> Value {
    cmd(
        "block.add",
        json!({"model": "models/pid.scmodel", "type": ty, "name": name, "params": params}),
    )
}

fn errors(check: &Value) -> Vec<&Value> {
    check["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == "error")
        .collect()
}

#[test]
fn an_agent_builds_runs_and_fixes_the_pid_example() {
    let dir = project("pid");
    let mut agent = Agent {
        server: Server::open(&dir).unwrap(),
        id: 0,
    };
    let init = agent.rpc(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(
        agent
            .server
            .handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none()
    );
    let tools = agent.rpc("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["check", "edit", "simulate", "signal_summary", "diff", "library_docs"]
    );

    // The agent reads the docs, then builds the loop, but forgets the feedback wire.
    let sum = agent.ok("library_docs", json!({"type": "core.Sum"}));
    assert_eq!(sum["default_input_names"], json!(["u1", "u2"]));
    let found = agent.ok("library_docs", json!({"query": "pid"}));
    assert!(found.as_array().unwrap().iter().any(|t| t["type"] == "core.PID"));
    let build = agent.ok(
        "edit",
        json!({"commands": [
            cmd("param.set", json!({"model": "models/pid.scmodel", "name": "Kp", "value": 2.0})),
            add("core.Constant", "ref", json!({"value": 1.0})),
            add("core.Sum", "error", json!({"signs": "+-"})),
            add("core.PID", "pid", json!({"P": "Kp", "I": 3.0})),
            add("core.TransferFcn", "plant", json!({"num": [1.0], "den": [0.5, 1.0]})),
            add("core.Outport", "y", json!({})),
            cmd("connect", json!({"source": "ref.y", "destination": "error.u1"})),
            cmd("connect", json!({"source": "error.y", "destination": "pid.u"})),
            cmd("connect", json!({"source": "pid.y", "destination": "plant.u"})),
            cmd("connect", json!({"source": "plant.y", "destination": "y.u"})),
        ]}),
    );
    assert_eq!(build["saved"], json!(["models/pid.scmodel"]));

    // check names the open input; the agent fixes it with a dry run first.
    let check = agent.ok("check", json!({}));
    assert!(!check["ok"].as_bool().unwrap());
    assert!(errors(&check).iter().any(|d| d["path"] == "error.u2"), "{check:#}");
    let wire = json!({"commands": [cmd("connect", json!({"source": "plant.y", "destination": "error.u2"}))]});
    let mut dry = wire.clone();
    dry["dry_run"] = json!(true);
    assert_eq!(agent.ok("edit", dry)["dry_run"], true);
    assert!(
        !agent.ok("check", json!({}))["ok"].as_bool().unwrap(),
        "a dry run changes nothing"
    );
    agent.ok("edit", wire);
    assert_eq!(agent.ok("check", json!({}))["ok"], true);

    // A failed edit names its command and changes nothing.
    let (failed, e) = agent.tool(
        "edit",
        json!({"commands": [add("core.Gain", "k", json!({})), add("core.Gain", "k", json!({}))]}),
    );
    assert!(failed && e["failed_command"] == 1, "{e:#}");
    let (failed, _) = agent.tool("edit", json!({"commands": [cmd("project.trust", json!({}))]}));
    assert!(failed, "project.trust is never an MCP edit");

    // Run it: the PI loop settles at the reference.
    let run = agent.ok("simulate", json!({"stop_time": 10.0}));
    let summary = agent.ok("signal_summary", json!({"run": run["run"], "points": 10}));
    let y = &summary["y"];
    assert!((y["final"][0].as_f64().unwrap() - 1.0).abs() < 1e-3, "{y:#}");
    assert!(y["trace"]["t"].as_array().unwrap().len() <= 11);

    // The file route: the agent breaks a parameter in the file, check finds it, the agent fixes the file.
    let file = dir.join("models/pid.scmodel");
    let good = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, good.replace("P = \"Kp\"", "P = \"Kq\"")).unwrap();
    let check = agent.ok("check", json!({"path": "models/pid.scmodel"}));
    assert!(errors(&check).iter().any(|d| d["path"] == "pid.P"), "{check:#}");
    std::fs::write(&file, &good).unwrap();
    assert_eq!(agent.ok("check", json!({"path": "models/pid.scmodel"}))["ok"], true);

    // A file that is not TOML: check gives the file diagnostics.
    std::fs::write(&file, "format = ").unwrap();
    assert!(!errors(&agent.ok("check", json!({}))).is_empty());
    std::fs::write(&file, &good).unwrap();

    // Models are resources.
    let list = agent.rpc("resources/list", json!({}));
    let uri = list["result"]["resources"][0]["uri"].as_str().unwrap().to_string();
    assert_eq!(uri, "simucraft://model/models/pid.scmodel");
    let read = agent.rpc("resources/read", json!({"uri": uri}));
    let model: Value = serde_json::from_str(read["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(model["model"]["blocks"].as_object().unwrap().len(), 5);

    // Paths stay inside the project.
    assert!(agent.tool("check", json!({"path": "../outside.scmodel"})).0);
}

#[test]
fn diff_against_a_git_ref_and_another_file() {
    let dir = project("diff");
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "empty model"]);
    let mut agent = Agent {
        server: Server::open(&dir).unwrap(),
        id: 0,
    };
    agent.ok("edit", json!({"commands": [add("core.Gain", "k", json!({"k": 2.0}))]}));
    let d = agent.ok("diff", json!({"path": "models/pid.scmodel"}));
    let changes = d["changes"].as_array().unwrap();
    assert!(!changes.is_empty() && changes.iter().all(|c| c["op"] == "add"), "{d:#}");
    assert!(
        agent
            .tool("diff", json!({"path": "models/pid.scmodel", "ref": "--output=x"}))
            .0
    );
    let same = agent.ok(
        "diff",
        json!({"path": "models/pid.scmodel", "other": "models/pid.scmodel"}),
    );
    assert_eq!(same["changes"], json!([]));
}

#[test]
fn the_binary_speaks_mcp_on_stdio() {
    let dir = project("stdio");
    let mut child = Command::new(env!("CARGO_BIN_EXE_simucraft-mcp"))
        .arg(Path::new(&dir))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05"}}}}"#
    )
    .unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#).unwrap();
    drop(stdin);
    let lines: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    assert!(child.wait().unwrap().success());
    assert_eq!(lines.len(), 2, "the notification gets no reply");
    assert_eq!(lines[0]["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(lines[1]["result"]["tools"].as_array().unwrap().len(), 6);
}
