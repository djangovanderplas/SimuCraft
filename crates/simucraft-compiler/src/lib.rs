//! The SimuCraft compiler (spec E1): model files to an [`ir::CompiledModel`].
//!
//! Passes, in order:
//! 1. Load the top model and every subsystem file; check each with
//!    `simucraft-format`; flatten subsystems (IDs become paths of IDs).
//! 2. Evaluate model and block parameters with the expression engine.
//! 3. Resolve every input to the output port that feeds it, through
//!    subsystem ports.
//! 4. Propagate widths: build each block with the widths of its sources,
//!    until the widths do not change.
//! 5. Sort by direct feedthrough; a cycle is an algebraic loop.
//! 6. Resolve inherited sample times and assign slots.
//!
//! Physical networks (nets, `core.Connector`) wait for the acausal compiler
//! (WP2.7): the compiler warns and skips them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use simucraft_contracts::Diagnostic;
use simucraft_contracts::block::{BlockType, Layout, Params, SampleTime};
use simucraft_contracts::ir::{self, BlockInstance, CompiledModel, ParamValue, Slots};
use simucraft_format::schema::{self, Model};

mod params;

use params::Scope;

/// Diagnostic codes of the compiler.
pub mod codes {
    /// The library has no block type of this name.
    pub const UNKNOWN_TYPE: &str = "C001";
    /// An input or output port that does not exist, or a source that cannot feed it.
    pub const PORT: &str = "C002";
    /// An input with no source.
    pub const UNCONNECTED: &str = "C003";
    /// An input whose width is not the width of its source.
    pub const WIDTH: &str = "C004";
    /// A cycle of blocks with direct feedthrough.
    pub const ALGEBRAIC_LOOP: &str = "C005";
    /// A parameter that is unknown, of the wrong kind, or cannot be evaluated.
    pub const PARAM: &str = "C006";
    /// A subsystem file that is missing, invalid, or refers back to itself.
    pub const SUBSYSTEM: &str = "C008";
    /// A physical network that this compiler does not compile yet (warning).
    pub const PHYSICAL: &str = "C009";
    /// A parameter whose expression unit does not match its declared unit.
    pub const UNIT: &str = "C010";
}

/// Compile the model file at `top`. `load` reads a file by path, so the
/// model service can compile unsaved text. Returns the compiled model if
/// there is no error, and all diagnostics.
pub fn compile(
    top: &Path,
    library: &[Box<dyn BlockType>],
    load: &dyn Fn(&Path) -> Option<String>,
) -> (Option<CompiledModel>, Vec<Diagnostic>) {
    let mut c = Compiler {
        library,
        load,
        instances: Vec::new(),
        kinds: BTreeMap::new(),
        flats: Vec::new(),
        diagnostics: Vec::new(),
    };
    match c.instance(top.to_path_buf(), None, BTreeMap::new(), &mut Vec::new()) {
        Ok(Some(_)) => {}
        Ok(None) => return (None, c.diagnostics),
        Err(d) => {
            c.diagnostics.push(d.at(top.display().to_string()));
            return (None, c.diagnostics);
        }
    }
    let model = c.link();
    (model, c.diagnostics)
}

/// One loaded model file: the top model or one subsystem instance.
struct Instance {
    model: Model,
    file: String,
    /// Parent instance and the ID of the subsystem block there.
    parent: Option<(usize, String)>,
    /// Prefixes for flattened IDs (`b_x/`) and names (`motor/`).
    ids: String,
    names: String,
    scope: Scope,
}

/// What a block ID in an instance stands for.
#[derive(Clone, Copy)]
enum Kind {
    /// A block that runs: index into `flats`.
    Real(usize),
    /// An inport of a subsystem file: it stands for the parent's source.
    Inport,
    /// A subsystem block: index of the child instance.
    Sub(usize),
    /// No signal outputs (a subsystem outport, a connector).
    Skip,
}

struct Flat {
    inst: usize,
    id: String,
    ty: usize,
    params: Params,
    ts: Option<f64>,
    ports: Vec<String>,
    /// Source (flat index, output port index) per input port.
    sources: Vec<(usize, usize)>,
    layout: Option<Layout>,
}

struct Compiler<'a> {
    library: &'a [Box<dyn BlockType>],
    load: &'a dyn Fn(&Path) -> Option<String>,
    instances: Vec<Instance>,
    kinds: BTreeMap<(usize, String), Kind>,
    flats: Vec<Flat>,
    diagnostics: Vec<Diagnostic>,
}

impl Compiler<'_> {
    fn spec(&self, f: &Flat) -> &simucraft_contracts::block::BlockSpec {
        self.library[f.ty].spec()
    }

    /// Diagnostic about block `id` of instance `inst`, at `<path>.<member>`.
    fn at(&self, d: Diagnostic, inst: usize, id: &str, member: Option<&str>) -> Diagnostic {
        let i = &self.instances[inst];
        let name = i.model.blocks.get(id).map_or(id, |b| b.name.as_str());
        let path = match member {
            Some(m) => format!("{}{name}.{m}", i.names),
            None => format!("{}{name}", i.names),
        };
        let mut d = d.id(id).at(path);
        d.file = Some(i.file.clone());
        d
    }

    /// Load, check and flatten one model file. `Ok(None)`: the file has errors (reported).
    fn instance(
        &mut self,
        path: PathBuf,
        parent: Option<(usize, String)>,
        overrides: BTreeMap<String, params::Entry>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<Option<usize>, Diagnostic> {
        let file = path.display().to_string();
        let text =
            (self.load)(&path).ok_or_else(|| Diagnostic::error(codes::SUBSYSTEM, format!("cannot read `{file}`")))?;
        let checked = simucraft_format::check(simucraft_format::Kind::Model, &text);
        let failed = checked.iter().any(Diagnostic::is_error);
        self.diagnostics.extend(checked.into_iter().map(|mut d| {
            d.file = Some(file.clone());
            d
        }));
        if failed {
            return Ok(None);
        }
        let model: Model = toml_edit::de::from_str(&text).expect("check() accepted this model");
        let (ids, names) = match &parent {
            Some((p, sub)) => {
                let pi = &self.instances[*p];
                (
                    format!("{}{sub}/", pi.ids),
                    format!("{}{}/", pi.names, pi.model.blocks[sub].name),
                )
            }
            None => (String::new(), String::new()),
        };
        for name in overrides.keys() {
            if !model.parameters.contains_key(name) {
                let (p, sub) = parent.clone().expect("only subsystems have overrides");
                let d = Diagnostic::error(codes::PARAM, format!("`{file}` has no parameter `{name}`"));
                self.diagnostics.push(self.at(d, p, &sub, Some(name)));
            }
        }
        if !model.nets.is_empty() {
            let mut d = Diagnostic::warning(
                codes::PHYSICAL,
                "physical connections (nets) are not compiled before WP2.7; signals from physical blocks read 0",
            );
            d.file = Some(file.clone());
            self.diagnostics.push(d);
        }
        let inst = self.instances.len();
        let blocks: Vec<(String, schema::Block)> =
            model.blocks.iter().map(|(k, b)| (k.clone(), clone_block(b))).collect();
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let is_top = parent.is_none();
        self.instances.push(Instance {
            scope: Scope::new(&model, overrides),
            model,
            file,
            parent,
            ids,
            names,
        });
        stack.push(path);
        for (id, b) in blocks {
            let kind = match b.kind.as_str() {
                "subsystem" => {
                    let child = dir.join(b.reference.as_deref().unwrap_or_default());
                    if stack.contains(&child) {
                        let d =
                            Diagnostic::error(codes::SUBSYSTEM, format!("`{}` refers back to itself", child.display()));
                        self.diagnostics.push(self.at(d, inst, &id, None));
                        continue;
                    }
                    let mut overrides = BTreeMap::new();
                    for (name, v) in &b.params {
                        match self.instances[inst].scope.literal_or_expr(v) {
                            Ok(entry) => {
                                overrides.insert(name.clone(), entry);
                            }
                            Err(d) => self.diagnostics.push(self.at(d, inst, &id, Some(name))),
                        }
                    }
                    match self.instance(child, Some((inst, id.clone())), overrides, stack) {
                        Ok(Some(c)) => Kind::Sub(c),
                        Ok(None) => continue,
                        Err(d) => {
                            self.diagnostics.push(self.at(d, inst, &id, None));
                            continue;
                        }
                    }
                }
                "core.Connector" => Kind::Skip,
                "core.Inport" if !is_top => Kind::Inport,
                "core.Outport" if !is_top => Kind::Skip,
                _ => match self.real(inst, &id, &b) {
                    Some(f) => Kind::Real(f),
                    None => continue,
                },
            };
            self.kinds.insert((inst, id), kind);
        }
        stack.pop();
        Ok(Some(inst))
    }

    /// A block that runs: look up its type and evaluate its parameters.
    fn real(&mut self, inst: usize, id: &str, b: &schema::Block) -> Option<usize> {
        let Some(ty) = self.library.iter().position(|t| t.spec().type_name == b.kind) else {
            let d = Diagnostic::error(codes::UNKNOWN_TYPE, format!("unknown block type `{}`", b.kind));
            self.diagnostics.push(self.at(d, inst, id, None));
            return None;
        };
        let spec = self.library[ty].spec();
        let mut params = Params::new();
        let mut ok = true;
        for (name, v) in &b.params {
            let Some(p) = spec.params.iter().find(|p| &p.name == name) else {
                let d = Diagnostic::error(codes::PARAM, format!("`{}` has no parameter `{name}`", b.kind));
                self.diagnostics.push(self.at(d, inst, id, Some(name)));
                ok = false;
                continue;
            };
            match self.instances[inst].scope.block_param(v, p.default.as_deref()) {
                Ok(value) => {
                    params.insert(name.clone(), value);
                }
                Err(d) => {
                    self.diagnostics.push(self.at(d, inst, id, Some(name)));
                    ok = false;
                }
            }
        }
        if let Some(ts) = b.ts
            && spec.params.iter().any(|p| p.name == "ts")
        {
            params.insert("ts".into(), ParamValue::Scalar(ts));
        }
        // `core.Sum` has one input per sign.
        let ports = match (spec.type_name.as_str(), params.get("signs")) {
            ("core.Sum", Some(ParamValue::Text(signs))) => (1..=signs.len()).map(|i| format!("u{i}")).collect(),
            _ => spec.inputs.iter().map(|p| p.name.clone()).collect(),
        };
        ok.then(|| {
            self.flats.push(Flat {
                inst,
                id: id.to_string(),
                ty,
                params,
                ts: b.ts,
                ports,
                sources: Vec::new(),
                layout: None,
            });
            self.flats.len() - 1
        })
    }

    /// The flat block and output port index that `<id>.<port>` in `inst` stands for.
    fn resolve(&self, inst: usize, source: &str, depth: usize) -> Result<(usize, usize), String> {
        let (id, port) = source
            .split_once('.')
            .ok_or_else(|| format!("`{source}` is not `<block>.<port>`"))?;
        let i = &self.instances[inst];
        let name = i.model.blocks.get(id).map_or(id, |b| b.name.as_str());
        if depth > 64 {
            return Err(format!("`{name}.{port}` is part of a loop of subsystem ports"));
        }
        match self.kinds.get(&(inst, id.to_string())) {
            None => Err(format!("`{name}` did not compile")),
            Some(Kind::Real(f)) => {
                let outputs = &self.spec(&self.flats[*f]).outputs;
                let at = outputs.iter().position(|o| o.name == port);
                at.map(|o| (*f, o))
                    .ok_or_else(|| format!("`{name}` has no output `{port}`"))
            }
            Some(Kind::Inport) => {
                let (p, sub) = i.parent.as_ref().expect("an inport alias is in a subsystem");
                let parent = &self.instances[*p].model.blocks[sub];
                match parent.inputs.get(name) {
                    Some(src) if port == "y" => self.resolve(*p, src, depth + 1),
                    Some(_) => Err(format!("inport `{name}` has only the output `y`")),
                    None => Err(format!("subsystem input `{}.{name}` has no source", parent.name)),
                }
            }
            Some(Kind::Sub(child)) => {
                let model = &self.instances[*child].model;
                let outport = model
                    .blocks
                    .values()
                    .find(|b| b.kind == "core.Outport" && b.name == port);
                match outport.map(|b| b.inputs.get("u")) {
                    Some(Some(src)) => self.resolve(*child, src, depth + 1),
                    Some(None) => Err(format!("outport `{port}` inside `{name}` has no source")),
                    None => Err(format!("subsystem `{name}` has no output `{port}`")),
                }
            }
            Some(Kind::Skip) => Err(format!("`{name}` has no signal outputs")),
        }
    }

    /// Passes 3 to 6.
    fn link(&mut self) -> Option<CompiledModel> {
        self.connect();
        if self.diagnostics.iter().any(Diagnostic::is_error) {
            return None;
        }
        self.widths();
        if self.diagnostics.iter().any(Diagnostic::is_error) {
            return None;
        }
        let order = self.sort()?;
        Some(self.emit(&order))
    }

    fn connect(&mut self) {
        for f in 0..self.flats.len() {
            let (inst, id) = (self.flats[f].inst, self.flats[f].id.clone());
            let inputs = self.instances[inst].model.blocks[&id].inputs.clone();
            for port in inputs.keys().filter(|k| !self.flats[f].ports.contains(k)) {
                let d = Diagnostic::error(codes::PORT, format!("this block has no input `{port}`"));
                self.diagnostics.push(self.at(d, inst, &id, Some(port)));
            }
            let mut sources = Vec::new();
            for port in self.flats[f].ports.clone() {
                let result = match inputs.get(&port) {
                    None => Err((codes::UNCONNECTED, "this input has no source".to_string())),
                    Some(src) => self.resolve(inst, src, 0).map_err(|m| (codes::PORT, m)),
                };
                match result {
                    Ok(s) => sources.push(s),
                    Err((code, m)) => {
                        let d = self.at(Diagnostic::error(code, m), inst, &id, Some(&port));
                        self.diagnostics.push(d);
                    }
                }
            }
            self.flats[f].sources = sources;
        }
    }

    /// Build every block with the widths of its sources until nothing changes.
    /// Width 1 stands in for a source that has no layout yet.
    fn widths(&mut self) {
        let mut results: Vec<Result<Layout, Diagnostic>> = Vec::new();
        for _ in 0..16 {
            let mut changed = false;
            results.clear();
            for f in 0..self.flats.len() {
                let widths: Vec<usize> = self.flats[f]
                    .sources
                    .iter()
                    .map(|(s, o)| self.flats[*s].layout.as_ref().map_or(1, |l| l.outputs[*o]))
                    .collect();
                let built = self.library[self.flats[f].ty]
                    .build(&self.flats[f].params, &widths)
                    .map(|(_, l)| l);
                if let Ok(l) = &built
                    && self.flats[f].layout.as_ref().is_none_or(|old| old.outputs != l.outputs)
                {
                    self.flats[f].layout = Some(l.clone());
                    changed = true;
                }
                results.push(built);
            }
            if !changed {
                break;
            }
        }
        for (f, result) in results.into_iter().enumerate() {
            let flat = &self.flats[f];
            match result {
                Err(d) => self.diagnostics.push(self.at(d, flat.inst, &flat.id, None)),
                Ok(l) => {
                    let wrong = l.outputs.len() != self.spec(flat).outputs.len() || l.inputs.len() != flat.ports.len();
                    if wrong {
                        let d = Diagnostic::error(codes::PORT, "the block's layout does not match its ports");
                        self.diagnostics.push(self.at(d, flat.inst, &flat.id, None));
                        continue;
                    }
                    for (i, (s, o)) in flat.sources.iter().enumerate() {
                        let have = self.flats[*s].layout.as_ref().map_or(0, |l| l.outputs[*o]);
                        if have != l.inputs[i] {
                            let m = format!("input width {} does not match its source width {have}", l.inputs[i]);
                            let d = self.at(
                                Diagnostic::error(codes::WIDTH, m),
                                flat.inst,
                                &flat.id,
                                Some(&flat.ports[i]),
                            );
                            self.diagnostics.push(d);
                        }
                    }
                }
            }
        }
    }

    /// Execution order: a block runs after the sources of its feedthrough inputs.
    fn sort(&mut self) -> Option<Vec<usize>> {
        let feeds = |f: &Flat| -> Vec<(usize, usize)> {
            let l = f.layout.as_ref().expect("built");
            f.sources
                .iter()
                .enumerate()
                .filter(|(i, _)| l.feedthrough[*i])
                .map(|(i, (s, _))| (i, *s))
                .collect()
        };
        let edges: Vec<Vec<(usize, usize)>> = self.flats.iter().map(feeds).collect();
        let mut waiting: Vec<usize> = edges.iter().map(Vec::len).collect();
        let mut users: Vec<Vec<usize>> = vec![Vec::new(); self.flats.len()];
        for (f, e) in edges.iter().enumerate() {
            e.iter().for_each(|(_, s)| users[*s].push(f));
        }
        let mut ready: BTreeSet<usize> = (0..self.flats.len()).filter(|f| waiting[*f] == 0).collect();
        let mut order = Vec::new();
        while let Some(f) = ready.pop_first() {
            order.push(f);
            for &u in &users[f] {
                waiting[u] -= 1;
                if waiting[u] == 0 {
                    ready.insert(u);
                }
            }
        }
        if order.len() == self.flats.len() {
            return Some(order);
        }
        // Walk back along feedthrough inputs from the first unsorted block until a block repeats.
        let mut path: Vec<(usize, usize)> = Vec::new();
        let mut f = (0..self.flats.len())
            .find(|f| waiting[*f] > 0)
            .expect("a block is unsorted");
        while !path.iter().any(|(g, _)| *g == f) {
            let &(port, src) = edges[f]
                .iter()
                .find(|(_, s)| waiting[*s] > 0)
                .expect("an unsorted source");
            path.push((f, port));
            f = src;
        }
        let start = path.iter().position(|(g, _)| *g == f).expect("the repeated block");
        let cycle = &path[start..];
        let name = |(g, p): &(usize, usize)| {
            let flat = &self.flats[*g];
            let i = &self.instances[flat.inst];
            format!("{}{}.{}", i.names, i.model.blocks[&flat.id].name, flat.ports[*p])
        };
        let names: Vec<String> = cycle.iter().map(name).collect();
        let m = format!(
            "algebraic loop through {}; add a block without direct feedthrough (for example a delay)",
            names.join(", ")
        );
        let (g, p) = cycle[0];
        let flat = &self.flats[g];
        let d = self.at(
            Diagnostic::error(codes::ALGEBRAIC_LOOP, m),
            flat.inst,
            &flat.id,
            Some(&flat.ports[p]),
        );
        self.diagnostics.push(d);
        None
    }

    fn emit(&self, order: &[usize]) -> CompiledModel {
        let mut m = CompiledModel {
            format: ir::FORMAT.into(),
            signals: 0,
            continuous_states: 0,
            discrete_states: 0,
            algebraic: 0,
            zero_crossings: 0,
            rates: Vec::new(),
            blocks: Vec::new(),
            inports: BTreeMap::new(),
            outports: BTreeMap::new(),
            logged: BTreeMap::new(),
        };
        let mut outputs: Vec<Vec<Slots>> = vec![Vec::new(); self.flats.len()];
        let mut rates: Vec<SampleTime> = vec![SampleTime::Continuous; self.flats.len()];
        let mut done = vec![false; self.flats.len()];
        for &f in order {
            let flat = &self.flats[f];
            let l = flat.layout.as_ref().expect("built");
            outputs[f] = l
                .outputs
                .iter()
                .map(|w| {
                    m.signals += w;
                    Slots::new(m.signals - w, *w)
                })
                .collect();
        }
        let take = |counter: &mut usize, n: usize| {
            *counter += n;
            Slots::new(*counter - n, n)
        };
        for &f in order {
            let flat = &self.flats[f];
            let l = flat.layout.as_ref().expect("built");
            let rate = match flat.ts {
                Some(ts) if ts > 0.0 => SampleTime::Discrete {
                    period: ts,
                    offset: 0.0,
                },
                Some(0.0) => SampleTime::Continuous,
                _ => l.sample_time,
            };
            rates[f] = match rate {
                SampleTime::Inherited => {
                    let from = flat.sources.iter().map(|(s, _)| {
                        let own = self.flats[*s].layout.as_ref().expect("built").sample_time;
                        if done[*s] {
                            rates[*s]
                        } else if own == SampleTime::Inherited {
                            SampleTime::Continuous
                        } else {
                            own
                        }
                    });
                    inherit(from)
                }
                r => r,
            };
            done[f] = true;
            let index = m.rates.iter().position(|r| *r == rates[f]).unwrap_or_else(|| {
                m.rates.push(rates[f]);
                m.rates.len() - 1
            });
            let i = &self.instances[flat.inst];
            m.blocks.push(BlockInstance {
                id: format!("{}{}", i.ids, flat.id),
                type_name: self.spec(flat).type_name.clone(),
                params: flat.params.clone(),
                inputs: flat.sources.iter().map(|(s, o)| outputs[*s][*o]).collect(),
                outputs: outputs[f].clone(),
                x: take(&mut m.continuous_states, l.continuous_states),
                xd: take(&mut m.discrete_states, l.discrete_states),
                z: take(&mut m.algebraic, l.algebraic),
                zc: take(&mut m.zero_crossings, l.zero_crossings),
                rate: index,
            });
            let path = format!("{}{}", i.names, i.model.blocks[&flat.id].name);
            match self.spec(flat).type_name.as_str() {
                "core.Inport" if flat.inst == 0 => {
                    m.inports.insert(path, outputs[f][0]);
                }
                "core.Outport" if flat.inst == 0 => {
                    let (s, o) = flat.sources[0];
                    m.outports.insert(path, outputs[s][o]);
                }
                "core.Scope" => {
                    let (s, o) = flat.sources[0];
                    m.logged.insert(path, outputs[s][o]);
                }
                _ => {}
            }
        }
        for (inst, i) in self.instances.iter().enumerate() {
            for (key, signal) in i.model.signals.iter().filter(|(_, s)| s.log) {
                if let Ok((s, o)) = self.resolve(inst, key, 0) {
                    m.logged.insert(
                        format!("{}{}", i.names, signal.name.as_deref().unwrap_or(key)),
                        outputs[s][o],
                    );
                }
            }
        }
        m
    }
}

/// The rate of an inherited block from the rates of its sources: continuous
/// if any source is, else the one discrete rate, else constant.
fn inherit(sources: impl Iterator<Item = SampleTime>) -> SampleTime {
    let mut out = SampleTime::Constant;
    let mut any = false;
    for r in sources {
        any = true;
        out = match (out, r) {
            (SampleTime::Continuous, _) | (_, SampleTime::Continuous | SampleTime::Inherited) => SampleTime::Continuous,
            (SampleTime::Constant, r) | (r, SampleTime::Constant) => r,
            // shortcut: mixed discrete rates take the faster one; rate transitions come with WP2.1.
            (SampleTime::Discrete { period: a, .. }, SampleTime::Discrete { period: b, .. }) => {
                if a <= b {
                    out
                } else {
                    r
                }
            }
            (SampleTime::Inherited, r) => r,
        };
    }
    if any { out } else { SampleTime::Continuous }
}

fn clone_block(b: &schema::Block) -> schema::Block {
    schema::Block {
        name: b.name.clone(),
        kind: b.kind.clone(),
        reference: b.reference.clone(),
        ts: b.ts,
        doc: b.doc.clone(),
        trace: b.trace.clone(),
        params: b.params.clone(),
        inputs: b.inputs.clone(),
    }
}
