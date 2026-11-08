//! The model service (spec Section 8): it holds one open project and runs
//! Model API commands on its model files.
//!
//! Every edit runs on a copy of the files. The service checks each changed
//! file with `simucraft-format`; an edit that makes a file invalid is refused
//! with the file's diagnostics. The semantic diff is the difference between
//! the file's data before and after the edit. Undo, redo and transactions
//! keep snapshots of all files.
//!
//! Names resolve in the top model; `a/b` goes into subsystem `a`. IDs
//! (`b_…`) resolve in any open model file.
//!
//! `compile` and `simulate` use the files in memory, so unsaved edits count.
//! A run keeps its traces in memory until the service stops; `signal.get`
//! reads them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde_json::{Value as Json, json};
use simucraft_contracts::Diagnostic;
use simucraft_contracts::api::{
    Change, ChangeOp, Command, Edit, EditResult, Outcome, Request, Response, RpcError, SignalMode, Simulate,
};
use simucraft_contracts::ir::{CompiledModel, Slots};
use simucraft_format::{File, Kind, id, schema};
use simucraft_runtime::{Bdf, DormandPrince, Euler, Rk4, Simulation, Solver, Traces};

/// Diagnostic codes of the service.
pub mod codes {
    /// A model, block or port that the project does not have.
    pub const NOT_FOUND: &str = "A001";
    /// A command that does not fit the session state (no project, no transaction).
    pub const STATE: &str = "A002";
    /// Arguments that are not valid for the command.
    pub const ARGS: &str = "A003";
    /// A file that cannot be read or written.
    pub const IO: &str = "A004";
}

type Files = BTreeMap<String, File>;

/// One open project and its edit history.
#[derive(Default)]
pub struct Service {
    root: Option<PathBuf>,
    top: String,
    files: Files,
    dirty: BTreeSet<String>,
    undo: Vec<(Files, BTreeSet<String>)>,
    redo: Vec<(Files, BTreeSet<String>)>,
    /// Snapshot at `tx.begin`, while a transaction is open.
    tx: Option<(Files, BTreeSet<String>)>,
    /// Finished runs; run `r<n>` is at index `n - 1`.
    runs: Vec<Traces>,
}

/// Most points that `signal.get` gives in `decimated` mode.
const DECIMATED_POINTS: usize = 1000;

fn fail(code: &str, message: impl Into<String>) -> RpcError {
    let message = message.into();
    RpcError {
        code: 1,
        diagnostics: vec![Diagnostic::error(code, message.clone())],
        message,
    }
}

impl Service {
    /// Run one request.
    pub fn handle(&mut self, request: Request) -> Response {
        let outcome = match self.dispatch(request.command) {
            Ok(v) => Outcome::Result(v),
            Err(e) => Outcome::Error(e),
        };
        Response {
            jsonrpc: "2.0".into(),
            id: request.id,
            outcome,
        }
    }

    /// Run one request in its JSON text form and return the response as JSON text.
    pub fn handle_json(&mut self, text: &str) -> String {
        let response = match serde_json::from_str::<Request>(text) {
            Ok(request) => self.handle(request),
            Err(e) => Response {
                jsonrpc: "2.0".into(),
                id: serde_json::from_str::<Json>(text)
                    .ok()
                    .and_then(|v| v.get("id").cloned()),
                outcome: Outcome::Error(RpcError {
                    code: -32602,
                    message: e.to_string(),
                    diagnostics: vec![],
                }),
            },
        };
        serde_json::to_string(&response).expect("responses serialize")
    }

    fn dispatch(&mut self, command: Command) -> Result<Json, RpcError> {
        if self.root.is_none() && !matches!(command, Command::ProjectCreate { .. } | Command::ProjectOpen { .. }) {
            return Err(fail(
                codes::STATE,
                "no project is open; send project.open or project.create first",
            ));
        }
        match command {
            Command::ProjectCreate { path, name } => self.create(Path::new(&path), &name),
            Command::ProjectOpen { path } => self.open(Path::new(&path)),
            Command::ProjectSave {} => self.save(),
            Command::ProjectStatus {} => {
                let diagnostics: Vec<Diagnostic> = self
                    .dirty
                    .iter()
                    .flat_map(|f| {
                        simucraft_format::check(Kind::Model, &self.files[f].to_string())
                            .into_iter()
                            .map(move |mut d| {
                                d.file = Some(f.clone());
                                d
                            })
                    })
                    .collect();
                Ok(json!({"unsaved": self.dirty, "diagnostics": diagnostics}))
            }
            Command::BlockAdd(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let file = w.model(&args.model)?;
                let bid = id::new('b');
                let f = w.files.get_mut(&file).expect("resolved");
                f.set(&["blocks", &bid], "name", args.name.as_str());
                f.set(&["blocks", &bid], "type", args.type_name.as_str());
                for (k, v) in &args.params {
                    f.set(&["blocks", &bid, "params"], k, toml(v)?);
                }
                for (port, src) in &args.inputs {
                    let (sf, sid, out) = w.port(src)?;
                    same_file(&sf, &file)?;
                    let f = w.files.get_mut(&file).expect("resolved");
                    f.set(&["blocks", &bid, "inputs"], port, format!("{sid}.{out}"));
                }
                Ok(vec![bid])
            }),
            Command::BlockRemove(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (file, bid) = w.block(&args.target)?;
                let data = w.json(&file);
                let f = w.files.get_mut(&file).expect("resolved");
                f.remove(&["blocks", &bid]);
                // Inputs and signal entries that name the block go with it.
                let prefix = format!("{bid}.");
                for (other, b) in data["blocks"].as_object().into_iter().flatten() {
                    for (port, src) in b["inputs"].as_object().into_iter().flatten() {
                        if src.as_str().is_some_and(|s| s.starts_with(&prefix)) {
                            f.remove(&["blocks", other, "inputs", port]);
                        }
                    }
                }
                for key in data["signals"].as_object().into_iter().flatten().map(|(k, _)| k) {
                    if key.starts_with(&prefix) {
                        f.remove(&["signals", key]);
                    }
                }
                Ok(vec![])
            }),
            Command::BlockRename(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (file, bid) = w.block(&args.target)?;
                w.files
                    .get_mut(&file)
                    .expect("resolved")
                    .set(&["blocks", &bid], "name", args.name.as_str());
                Ok(vec![])
            }),
            Command::BlockSetParams(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (file, bid) = w.block(&args.target)?;
                let f = w.files.get_mut(&file).expect("resolved");
                for (k, v) in &args.params {
                    if v.is_null() {
                        f.remove(&["blocks", &bid, "params", k]);
                    } else {
                        f.set(&["blocks", &bid, "params"], k, toml(v)?);
                    }
                }
                Ok(vec![])
            }),
            Command::Connect(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (sf, sid, out) = w.port(&args.source)?;
                let (df, did, input) = w.port(&args.destination)?;
                same_file(&sf, &df)?;
                w.files.get_mut(&df).expect("resolved").set(
                    &["blocks", &did, "inputs"],
                    &input,
                    format!("{sid}.{out}"),
                );
                Ok(vec![])
            }),
            Command::Disconnect(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (file, bid, input) = w.port(&args.destination)?;
                if !w
                    .files
                    .get_mut(&file)
                    .expect("resolved")
                    .remove(&["blocks", &bid, "inputs", &input])
                {
                    return Err(fail(codes::NOT_FOUND, format!("`{}` has no source", args.destination)));
                }
                Ok(vec![])
            }),
            Command::ParamSet(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let file = w.model(&args.model)?;
                let f = w.files.get_mut(&file).expect("resolved");
                let table = ["parameters", args.name.as_str()];
                let given = [args.value.is_some(), args.expr.is_some(), args.file.is_some()];
                if given.iter().any(|g| *g) {
                    // A new source replaces the old one: a parameter has exactly one.
                    ["value", "expr", "file", "key"].iter().for_each(|k| {
                        f.remove(&["parameters", &args.name, k]);
                    });
                }
                if let Some(v) = &args.value {
                    f.set(&table, "value", toml(v)?);
                }
                if let Some(e) = &args.expr {
                    f.set(&table, "expr", e.as_str());
                }
                if let Some(p) = &args.file {
                    f.set(&table, "file", p.as_str());
                }
                if let Some(u) = &args.unit {
                    f.set(&table, "unit", u.as_str());
                }
                if let Some(t) = args.tunable {
                    f.set(&table, "tunable", t);
                }
                if let Some(u) = &args.uncertain {
                    let fields = u
                        .as_object()
                        .ok_or_else(|| fail(codes::ARGS, "`uncertain` must be an object"))?;
                    f.remove(&["parameters", &args.name, "uncertain"]);
                    for (k, v) in fields {
                        f.set(&["parameters", &args.name, "uncertain"], k, toml(v)?);
                    }
                }
                Ok(vec![])
            }),
            Command::ParamRemove(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let file = w.model(&args.model)?;
                if !w
                    .files
                    .get_mut(&file)
                    .expect("resolved")
                    .remove(&["parameters", &args.name])
                {
                    return Err(fail(
                        codes::NOT_FOUND,
                        format!("`{}` has no parameter `{}`", args.model, args.name),
                    ));
                }
                Ok(vec![])
            }),
            Command::SignalSet(Edit { args, dry_run }) => self.edit(dry_run, |w| {
                let (file, bid, port) = w.port(&args.port)?;
                let key = format!("{bid}.{port}");
                let f = w.files.get_mut(&file).expect("resolved");
                let table = ["signals", key.as_str()];
                if let Some(n) = &args.name {
                    f.set(&table, "name", n.as_str());
                }
                if let Some(l) = args.log {
                    f.set(&table, "log", l);
                }
                if let Some(a) = args.analysis_point {
                    f.set(&table, "analysis_point", a);
                }
                if let Some(t) = &args.trace {
                    f.set(&table, "trace", toml(&json!(t))?);
                }
                Ok(vec![])
            }),
            Command::TxBegin { label: _ } => {
                if self.tx.is_some() {
                    return Err(fail(codes::STATE, "a transaction is already open"));
                }
                self.tx = Some((self.files.clone(), self.dirty.clone()));
                Ok(json!({}))
            }
            Command::TxCommit {} => {
                let before = self
                    .tx
                    .take()
                    .ok_or_else(|| fail(codes::STATE, "no transaction is open"))?;
                self.undo.push(before);
                self.redo.clear();
                Ok(json!({}))
            }
            Command::TxRollback {} => {
                (self.files, self.dirty) = self
                    .tx
                    .take()
                    .ok_or_else(|| fail(codes::STATE, "no transaction is open"))?;
                Ok(json!({}))
            }
            Command::Undo {} => self.step(true),
            Command::Redo {} => self.step(false),
            Command::ModelGet { path, depth: _ } => {
                let file = model(&self.files, if path.is_empty() { &self.top } else { &path })?;
                Ok(json!({"file": file, "model": data(&self.files[&file])}))
            }
            Command::Find { query } => {
                let q = query.to_lowercase();
                let mut out = Vec::new();
                for (file, f) in &self.files {
                    for (bid, b) in data(f)["blocks"].as_object().into_iter().flatten() {
                        let (name, ty) = (b["name"].as_str().unwrap_or(""), b["type"].as_str().unwrap_or(""));
                        if name.to_lowercase().contains(&q) || ty.to_lowercase().contains(&q) {
                            out.push(json!({"file": file, "id": bid, "name": name, "type": ty}));
                        }
                    }
                }
                Ok(Json::Array(out))
            }
            Command::LibraryList { filter } => {
                let q = filter.unwrap_or_default().to_lowercase();
                let types: Vec<Json> = simucraft_blocks::library()
                    .iter()
                    .map(|t| t.spec())
                    .filter(|s| s.type_name.to_lowercase().contains(&q) || s.doc.to_lowercase().contains(&q))
                    .map(|s| json!({"type": s.type_name, "doc": s.doc}))
                    .collect();
                Ok(Json::Array(types))
            }
            Command::LibraryDescribe { type_name } => {
                let library = simucraft_blocks::library();
                let spec = library
                    .iter()
                    .map(|t| t.spec())
                    .find(|s| s.type_name == type_name)
                    .ok_or_else(|| fail(codes::NOT_FOUND, format!("no block type `{type_name}`")))?;
                let mut out = json!(spec);
                // The ports a block gets with the default parameters, for variadic blocks.
                out["default_input_names"] = json!(spec.input_names(&Default::default()).unwrap_or_default());
                Ok(out)
            }
            Command::Compile { model } => {
                let (_, diagnostics) = self.compile(&model)?;
                Ok(json!({"diagnostics": diagnostics}))
            }
            Command::Simulate(args) => self.simulate(args),
            Command::SignalGet { run, signal, mode } => self.signal(&run, &signal, mode),
            other => {
                let method = serde_json::to_value(&other)
                    .ok()
                    .and_then(|v| v["method"].as_str().map(String::from));
                Err(RpcError {
                    code: -32601,
                    message: format!("`{}` is not implemented yet", method.unwrap_or_default()),
                    diagnostics: vec![],
                })
            }
        }
    }

    /// Run `apply` on a copy of the files, check and diff the result, and keep it unless `dry_run`.
    fn edit(
        &mut self,
        dry_run: bool,
        apply: impl FnOnce(&mut WorkMut) -> Result<Vec<String>, RpcError>,
    ) -> Result<Json, RpcError> {
        let mut work = WorkMut {
            top: self.top.clone(),
            files: self.files.clone(),
        };
        let created = apply(&mut work)?;
        let mut diff = Vec::new();
        let mut changed = Vec::new();
        for (name, f) in &work.files {
            let text = f.to_string();
            if self.files.get(name).map(File::to_string).as_deref() == Some(text.as_str()) {
                continue;
            }
            let diagnostics = simucraft_format::check(Kind::Model, &text);
            if diagnostics.iter().any(Diagnostic::is_error) {
                let diagnostics: Vec<Diagnostic> = diagnostics
                    .into_iter()
                    .map(|mut d| {
                        d.file = Some(name.clone());
                        d
                    })
                    .collect();
                return Err(RpcError {
                    code: 1,
                    message: format!("the edit would make `{name}` invalid: {}", diagnostics[0].message),
                    diagnostics,
                });
            }
            json_diff(name, "", &data(&self.files[name]), &data(f), &mut diff);
            changed.push(name.clone());
        }
        if !dry_run && !changed.is_empty() {
            if self.tx.is_none() {
                self.undo.push((self.files.clone(), self.dirty.clone()));
                self.redo.clear();
            }
            self.files = work.files;
            self.dirty.extend(changed);
        }
        Ok(json!(EditResult {
            diff,
            created,
            diagnostics: vec![]
        }))
    }

    /// Compile a model file from the files in memory. Files that are not open
    /// (outside the project) come from the disk.
    fn compile(&self, name: &str) -> Result<(Option<CompiledModel>, Vec<Diagnostic>), RpcError> {
        let file = model(&self.files, if name.is_empty() { &self.top } else { name })?;
        let root = self.root.as_ref().expect("a project is open");
        let load = |p: &Path| match p
            .strip_prefix(root)
            .ok()
            .and_then(|rel| self.files.get(&normalize(rel)))
        {
            Some(f) => Some(f.to_string()),
            None => std::fs::read_to_string(p).ok(),
        };
        Ok(simucraft_compiler::compile(
            &root.join(&file),
            &simucraft_blocks::library(),
            &load,
        ))
    }

    /// Compile and run a model with the settings of its `[simulation]` table.
    fn simulate(&mut self, args: Simulate) -> Result<Json, RpcError> {
        if !args.overrides.is_empty() {
            return Err(fail(codes::ARGS, "`overrides` are not supported yet (WP2.4)"));
        }
        let (compiled, diagnostics) = self.compile(&args.model)?;
        let Some(compiled) = compiled else {
            let first = diagnostics
                .iter()
                .find(|d| d.is_error())
                .map_or("", |d| d.message.as_str());
            return Err(RpcError {
                code: 1,
                message: format!("the model does not compile: {first}"),
                diagnostics,
            });
        };
        let file = model(&self.files, if args.model.is_empty() { &self.top } else { &args.model })?;
        let settings: schema::Simulation = match data(&self.files[&file]).get("simulation") {
            Some(s) => {
                serde_json::from_value(s.clone()).map_err(|e| fail(codes::ARGS, format!("[simulation]: {e}")))?
            }
            None => schema::Simulation::default(),
        };
        if settings.start_time.is_some_and(|t| t != 0.0) {
            return Err(fail(codes::ARGS, "a start time other than 0 is not supported yet"));
        }
        let stop = args.stop_time.or(settings.stop_time).unwrap_or(10.0);
        let step = settings.fixed_step.or(settings.max_step).unwrap_or(stop / 1000.0);
        if !(stop > 0.0 && stop.is_finite() && step > 0.0 && step.is_finite()) {
            return Err(fail(codes::ARGS, "the stop time and the step must be positive numbers"));
        }
        let (rtol, atol) = (settings.rel_tol.unwrap_or(1e-6), settings.abs_tol.unwrap_or(1e-8));
        let mut solver: Box<dyn Solver> = match settings.solver.as_deref().unwrap_or("auto") {
            "auto" | "dormand_prince" => Box::new(DormandPrince::new(rtol, atol)),
            "bdf" => Box::new(Bdf { rtol, atol }),
            "rk4" => Box::new(Rk4 { h: step }),
            "euler" => Box::new(Euler { h: step }),
            other => {
                return Err(fail(
                    codes::ARGS,
                    format!("unknown solver `{other}`; use auto, dormand_prince, bdf, rk4 or euler"),
                ));
            }
        };
        // Outports and logged signals, by name; a logged signal wins over an outport of the same name.
        let mut available: BTreeMap<String, Slots> = compiled.outports.clone();
        available.extend(compiled.logged.clone());
        let recorded: BTreeMap<String, Slots> = if args.signals.is_empty() {
            available
        } else {
            args.signals
                .iter()
                .map(|n| match available.get(n) {
                    Some(s) => Ok((n.clone(), *s)),
                    None => Err(fail(
                        codes::NOT_FOUND,
                        format!("the model has no outport or logged signal `{n}`"),
                    )),
                })
                .collect::<Result<_, _>>()?
        };
        let to_error = |d: Diagnostic| RpcError {
            code: 1,
            message: d.message.clone(),
            diagnostics: vec![d],
        };
        let mut sim = Simulation::new(&compiled, &simucraft_blocks::library()).map_err(to_error)?;
        let mut traces = Traces {
            t: Vec::new(),
            signals: recorded.keys().map(|k| (k.clone(), Vec::new())).collect(),
        };
        sim.stream(solver.as_mut(), stop, step, &mut |t, signals| {
            traces.t.push(t);
            for (name, s) in &recorded {
                traces
                    .signals
                    .get_mut(name)
                    .expect("one trace per signal")
                    .extend(&signals[s.range()]);
            }
        })
        .map_err(to_error)?;
        let samples = traces.t.len();
        self.runs.push(traces);
        Ok(json!({
            "run": format!("r{}", self.runs.len()),
            "signals": recorded.keys().collect::<Vec<_>>(),
            "stop_time": stop,
            "samples": samples,
            "diagnostics": diagnostics,
        }))
    }

    /// One recorded signal of a run: all samples, at most [`DECIMATED_POINTS`], or statistics.
    fn signal(&self, run: &str, name: &str, mode: SignalMode) -> Result<Json, RpcError> {
        let traces = run
            .strip_prefix('r')
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| self.runs.get(n.checked_sub(1)?))
            .ok_or_else(|| fail(codes::NOT_FOUND, format!("no run `{run}`")))?;
        let values = traces
            .signals
            .get(name)
            .ok_or_else(|| fail(codes::NOT_FOUND, format!("run `{run}` has no signal `{name}`")))?;
        let width = values.len() / traces.t.len().max(1);
        Ok(match mode {
            SignalMode::Full => json!({"t": traces.t, "values": values, "width": width}),
            SignalMode::Decimated => {
                // shortcut: every k-th sample, so a short peak can fall out; use min-max decimation if plots need it.
                let stride = traces.t.len().div_ceil(DECIMATED_POINTS).max(1);
                let mut rows: Vec<usize> = (0..traces.t.len()).step_by(stride).collect();
                if rows.last() != Some(&(traces.t.len().saturating_sub(1))) && !traces.t.is_empty() {
                    rows.push(traces.t.len() - 1);
                }
                let t: Vec<f64> = rows.iter().map(|&r| traces.t[r]).collect();
                let v: Vec<f64> = rows
                    .iter()
                    .flat_map(|&r| values[r * width..(r + 1) * width].to_vec())
                    .collect();
                json!({"t": t, "values": v, "width": width})
            }
            SignalMode::Summary => {
                let at = |i: usize| traces.t[i / width.max(1)];
                let (imin, imax) = (0..values.len()).fold((0, 0), |(lo, hi), i| {
                    (
                        if values[i] < values[lo] { i } else { lo },
                        if values[i] > values[hi] { i } else { hi },
                    )
                });
                if values.is_empty() {
                    json!({"samples": 0, "width": width})
                } else {
                    json!({
                        "samples": traces.t.len(),
                        "width": width,
                        "min": values[imin], "t_min": at(imin),
                        "max": values[imax], "t_max": at(imax),
                        "mean": values.iter().sum::<f64>() / values.len() as f64,
                        "final": &values[values.len() - width..],
                        "t_final": traces.t.last(),
                    })
                }
            }
        })
    }

    fn step(&mut self, undo: bool) -> Result<Json, RpcError> {
        if self.tx.is_some() {
            return Err(fail(codes::STATE, "commit or roll back the open transaction first"));
        }
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let state = from
            .pop()
            .ok_or_else(|| fail(codes::STATE, if undo { "nothing to undo" } else { "nothing to redo" }))?;
        to.push((
            std::mem::replace(&mut self.files, state.0),
            std::mem::replace(&mut self.dirty, state.1),
        ));
        Ok(json!({}))
    }

    fn create(&mut self, root: &Path, name: &str) -> Result<Json, RpcError> {
        let io = |e: std::io::Error| fail(codes::IO, e.to_string());
        if root.join("simucraft.toml").exists() {
            return Err(fail(codes::ARGS, format!("`{}` already has a project", root.display())));
        }
        let top = format!("models/{name}.scmodel");
        let manifest = format!("format = \"simucraft.project/0.1\"\nname = \"{name}\"\ntop = \"{top}\"\n");
        let model = format!(
            "format = \"simucraft.model/0.1\"\nid = \"{}\"\nname = \"{name}\"\n",
            id::new('m')
        );
        for (text, kind) in [(&manifest, Kind::Project), (&model, Kind::Model)] {
            let d = simucraft_format::check(kind, text);
            if let Some(first) = d.first() {
                return Err(RpcError {
                    code: 1,
                    message: first.message.clone(),
                    diagnostics: d,
                });
            }
        }
        std::fs::create_dir_all(root.join("models")).map_err(io)?;
        std::fs::write(root.join("simucraft.toml"), manifest).map_err(io)?;
        std::fs::write(root.join(&top), model).map_err(io)?;
        self.open(root)
    }

    fn open(&mut self, root: &Path) -> Result<Json, RpcError> {
        let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| fail(codes::IO, format!("{}: {e}", p.display())));
        let manifest = read(&root.join("simucraft.toml"))?;
        let project = File::parse(Kind::Project, &manifest).map_err(|d| RpcError {
            code: 1,
            message: "simucraft.toml is not valid".into(),
            diagnostics: d,
        })?;
        let top = data(&project)["top"].as_str().unwrap_or_default().to_string();
        let mut files = Files::new();
        let mut skipped: Vec<Diagnostic> = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).map_err(|e| fail(codes::IO, e.to_string()))? {
                let path = entry.map_err(|e| fail(codes::IO, e.to_string()))?.path();
                let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
                if path.is_dir() && !hidden {
                    stack.push(path);
                } else if Kind::from_path(&path) == Some(Kind::Model) {
                    let rel = path
                        .strip_prefix(root)
                        .expect("under root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    // An invalid file stays closed; its diagnostics go back to the client.
                    match File::parse(Kind::Model, &read(&path)?) {
                        Ok(file) => {
                            files.insert(rel, file);
                        }
                        Err(d) => skipped.extend(d.into_iter().map(|mut d| {
                            d.file = Some(rel.clone());
                            d
                        })),
                    }
                }
            }
        }
        // Runs stay: a client can open the project again (to read edits on disk) and keep its run IDs.
        *self = Service {
            root: Some(root.to_path_buf()),
            top,
            files,
            runs: std::mem::take(&mut self.runs),
            ..Default::default()
        };
        let models: Vec<&String> = self.files.keys().collect();
        Ok(json!({"root": root, "top": self.top, "models": models, "diagnostics": skipped}))
    }

    fn save(&mut self) -> Result<Json, RpcError> {
        let root = self.root.clone().expect("checked in dispatch");
        if self.tx.is_some() {
            return Err(fail(codes::STATE, "commit or roll back the open transaction first"));
        }
        // shortcut: does not check for newer files on disk; file sync comes with WP2.9.
        let written: Vec<String> = std::mem::take(&mut self.dirty).into_iter().collect();
        for name in &written {
            let path = root.join(name);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| fail(codes::IO, e.to_string()))?;
            }
            std::fs::write(&path, self.files[name].to_string()).map_err(|e| fail(codes::IO, e.to_string()))?;
        }
        Ok(json!({ "written": written }))
    }
}

/// The files an edit works on.
struct WorkMut {
    top: String,
    files: Files,
}

impl WorkMut {
    fn model(&self, name: &str) -> Result<String, RpcError> {
        model(&self.files, name)
    }
    fn block(&self, target: &str) -> Result<(String, String), RpcError> {
        block(&self.top, &self.files, target)
    }
    fn port(&self, target: &str) -> Result<(String, String, String), RpcError> {
        let (b, p) = target
            .rsplit_once('.')
            .ok_or_else(|| fail(codes::ARGS, format!("`{target}` is not `<block>.<port>`")))?;
        let (file, bid) = self.block(b)?;
        Ok((file, bid, p.to_string()))
    }
    fn json(&self, file: &str) -> Json {
        data(&self.files[file])
    }
}

fn model(files: &Files, name: &str) -> Result<String, RpcError> {
    let name = name.trim_start_matches("./");
    files
        .contains_key(name)
        .then(|| name.to_string())
        .ok_or_else(|| fail(codes::NOT_FOUND, format!("the project has no model file `{name}`")))
}

/// A block by ID (any file) or by name path from the top model.
fn block(top: &str, files: &Files, target: &str) -> Result<(String, String), RpcError> {
    let missing = || fail(codes::NOT_FOUND, format!("no block `{target}`"));
    if id::is_valid(target, 'b') {
        return files
            .iter()
            .find(|(_, f)| data(f)["blocks"].get(target).is_some())
            .map(|(name, _)| (name.clone(), target.to_string()))
            .ok_or_else(missing);
    }
    let mut file = top.to_string();
    let parts: Vec<&str> = target.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let d = data(files.get(&file).ok_or_else(missing)?);
        let blocks = d["blocks"].as_object().ok_or_else(missing)?;
        let (bid, b) = blocks.iter().find(|(_, b)| b["name"] == *part).ok_or_else(missing)?;
        if i + 1 == parts.len() {
            return Ok((file, bid.clone()));
        }
        let reference = b["ref"].as_str().ok_or_else(missing)?;
        file = normalize(&Path::new(&file).parent().unwrap_or(Path::new("")).join(reference));
    }
    Err(missing())
}

/// `a/b/../c` to `a/c`, with `/` separators.
fn normalize(p: &Path) -> String {
    let mut out: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_string_lossy().into()),
            _ => {}
        }
    }
    out.join("/")
}

fn same_file(a: &str, b: &str) -> Result<(), RpcError> {
    if a == b {
        Ok(())
    } else {
        Err(fail(
            codes::ARGS,
            format!("a connection must stay in one model file, not `{a}` to `{b}`"),
        ))
    }
}

/// The data of a file as JSON.
fn data(f: &File) -> Json {
    toml_edit::de::from_str(&f.to_string()).expect("a checked file is valid TOML")
}

/// A JSON value as a TOML value. Objects are refused: files have no inline tables.
fn toml(v: &Json) -> Result<toml_edit::Value, RpcError> {
    Ok(match v {
        Json::Bool(b) => (*b).into(),
        Json::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64().expect("a JSON number").into(),
        },
        Json::String(s) => s.as_str().into(),
        Json::Array(items) => {
            let mut a = toml_edit::Array::new();
            for i in items {
                a.push(toml(i)?);
            }
            a.into()
        }
        Json::Null | Json::Object(_) => return Err(fail(codes::ARGS, format!("`{v}` cannot be a parameter value"))),
    })
}

/// Semantic diff of two JSON trees: one change per added, removed or changed key.
/// The semantic diff between two versions of a TOML project file: one
/// change for each key path that differs.
pub fn semantic_diff(file: &str, old: &str, new: &str) -> Result<Vec<Change>, String> {
    let parse = |text: &str, which: &str| {
        toml_edit::de::from_str::<Json>(text).map_err(|e| format!("{which} version of `{file}`: {e}"))
    };
    let mut out = Vec::new();
    json_diff(file, "", &parse(old, "the old")?, &parse(new, "the new")?, &mut out);
    Ok(out)
}

fn json_diff(file: &str, path: &str, old: &Json, new: &Json, out: &mut Vec<Change>) {
    let join = |k: &str| {
        if path.is_empty() {
            k.to_string()
        } else {
            format!("{path}.{k}")
        }
    };
    let change = |op, path: String, old: Option<&Json>, new: Option<&Json>| Change {
        op,
        file: file.into(),
        path,
        old: old.cloned(),
        new: new.cloned(),
    };
    match (old, new) {
        (Json::Object(a), Json::Object(b)) => {
            for (k, v) in a {
                match b.get(k) {
                    Some(w) => json_diff(file, &join(k), v, w, out),
                    None => out.push(change(ChangeOp::Remove, join(k), Some(v), None)),
                }
            }
            for (k, w) in b.iter().filter(|(k, _)| !a.contains_key(*k)) {
                out.push(change(ChangeOp::Add, join(k), None, Some(w)));
            }
        }
        (a, b) if a != b => out.push(change(ChangeOp::Set, path.to_string(), Some(a), Some(b))),
        _ => {}
    }
}
