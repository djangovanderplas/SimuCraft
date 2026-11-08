//! Fakes: the smallest working implementations of each contract. Lanes use
//! them in tests until the real component exists. Not for production.
//!
//! - [`library`]: four blocks (Constant, Gain, Sum, Integrator)
//! - [`run_euler`]: a fixed-step runtime over a [`CompiledModel`]
//! - [`Service`]: an in-memory Model API for a handful of commands

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::Diagnostic;
use crate::api::{self, Change, ChangeOp, Command, EditResult, Outcome, Request, Response, RpcError};
use crate::block::{Block, BlockSpec, BlockType, Built, Ctx, Layout, Params, PortSpec, SampleTime, VariadicInputs};
use crate::ir::{CompiledModel, ParamValue};

// ---------------------------------------------------------------- blocks

fn spec(type_name: &str, inputs: &[&str], params: &[&str]) -> BlockSpec {
    let port = |name: &&str| PortSpec {
        name: name.to_string(),
        doc: String::new(),
        dtype: Default::default(),
        width: None,
        unit: None,
    };
    BlockSpec {
        type_name: type_name.into(),
        doc: format!("Fake {type_name}"),
        params: params
            .iter()
            .map(|p| crate::block::ParamSpec {
                name: p.to_string(),
                doc: String::new(),
                default: None,
                unit: None,
                tunable: true,
            })
            .collect(),
        inputs: inputs.iter().map(port).collect(),
        variadic_inputs: None,
        outputs: [&"y"].into_iter().map(port).collect(),
        connectors: vec![],
    }
}

fn scalar(params: &Params, name: &str) -> Result<f64, Diagnostic> {
    params
        .get(name)
        .and_then(ParamValue::as_f64)
        .ok_or_else(|| Diagnostic::error("C001", format!("parameter `{name}` must be a number")))
}

fn layout(inputs: &[usize], feedthrough: bool, states: usize, sample_time: SampleTime) -> Layout {
    let width = inputs.first().copied().unwrap_or(1);
    Layout {
        inputs: inputs.to_vec(),
        outputs: vec![width],
        feedthrough: vec![feedthrough; inputs.len()],
        continuous_states: states,
        discrete_states: 0,
        algebraic: 0,
        zero_crossings: 0,
        sample_time,
    }
}

struct Fake {
    spec: BlockSpec,
    build: fn(&Params, &[usize]) -> Result<Built, Diagnostic>,
}

impl BlockType for Fake {
    fn spec(&self) -> &BlockSpec {
        &self.spec
    }
    fn build(&self, params: &Params, input_widths: &[usize], _: SampleTime) -> Result<Built, Diagnostic> {
        (self.build)(params, input_widths)
    }
}

struct Constant(f64);
impl Block for Constant {
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y.fill(self.0);
    }
}

struct Gain(f64);
impl Block for Gain {
    fn outputs(&mut self, cx: &mut Ctx) {
        for (y, u) in cx.y.iter_mut().zip(cx.u) {
            *y = self.0 * u;
        }
    }
}

/// Signs from `signs = "+-"`, encoded as a vector of +1 / -1.
struct Sum(Vec<f64>);
impl Block for Sum {
    fn outputs(&mut self, cx: &mut Ctx) {
        let width = cx.y.len();
        cx.y.fill(0.0);
        for (port, sign) in self.0.iter().enumerate() {
            for i in 0..width {
                cx.y[i] += sign * cx.u[cx.u_off[port] + i];
            }
        }
    }
}

struct Integrator(f64);
impl Block for Integrator {
    fn initialize(&mut self, x0: &mut [f64], _xd0: &mut [f64]) {
        x0.fill(self.0);
    }
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y.copy_from_slice(cx.x);
    }
    fn derivatives(&mut self, cx: &mut Ctx) {
        cx.dx.copy_from_slice(cx.u);
    }
}

/// `core.Constant` (`value`), `core.Gain` (`k`), `core.Sum` (`signs`, one
/// input per sign), `core.Integrator` (`x0`).
pub fn library() -> Vec<Box<dyn BlockType>> {
    vec![
        Box::new(Fake {
            spec: spec("core.Constant", &[], &["value"]),
            build: |p, w| {
                Ok((
                    Box::new(Constant(scalar(p, "value")?)),
                    layout(w, false, 0, SampleTime::Constant),
                ))
            },
        }),
        Box::new(Fake {
            spec: spec("core.Gain", &["u"], &["k"]),
            build: |p, w| {
                Ok((
                    Box::new(Gain(scalar(p, "k")?)),
                    layout(w, true, 0, SampleTime::Inherited),
                ))
            },
        }),
        Box::new(Fake {
            spec: BlockSpec {
                variadic_inputs: Some(VariadicInputs {
                    port: PortSpec {
                        name: "u".into(),
                        doc: String::new(),
                        dtype: Default::default(),
                        width: None,
                        unit: None,
                    },
                    count: "signs".into(),
                }),
                ..spec("core.Sum", &[], &["signs"])
            },
            build: |p, w| {
                let Some(ParamValue::Text(signs)) = p.get("signs") else {
                    return Err(Diagnostic::error("C001", "`signs` must be a string such as \"+-\""));
                };
                let signs: Vec<f64> = signs.chars().map(|c| if c == '-' { -1.0 } else { 1.0 }).collect();
                if signs.len() != w.len() {
                    return Err(Diagnostic::error("C002", "`signs` needs one sign per input"));
                }
                Ok((Box::new(Sum(signs)), layout(w, true, 0, SampleTime::Inherited)))
            },
        }),
        Box::new(Fake {
            spec: spec("core.Integrator", &["u"], &["x0"]),
            build: |p, w| {
                Ok((
                    Box::new(Integrator(scalar(p, "x0")?)),
                    layout(w, false, w[0], SampleTime::Continuous),
                ))
            },
        }),
    ]
}

// ---------------------------------------------------------------- runtime

/// Logged signal name -> one sample per step (including t = 0).
pub type Traces = BTreeMap<String, Vec<f64>>;

/// Run `model` with forward Euler. Checks that each block's [`Layout`] agrees
/// with the slots the compiled model gives it: that is the block/IR contract.
pub fn run_euler(
    model: &CompiledModel,
    library: &[Box<dyn BlockType>],
    dt: f64,
    t_end: f64,
) -> Result<Traces, Diagnostic> {
    let mut blocks = Vec::with_capacity(model.blocks.len());
    let mut x = vec![0.0; model.continuous_states];
    let mut xd = vec![0.0; model.discrete_states];
    for inst in &model.blocks {
        let ty = library
            .iter()
            .find(|t| t.spec().type_name == inst.type_name)
            .ok_or_else(|| {
                Diagnostic::error("R001", format!("unknown block type `{}`", inst.type_name)).id(&inst.id)
            })?;
        let widths: Vec<usize> = inst.inputs.iter().map(|s| s.len).collect();
        let rate = *model
            .rates
            .get(inst.rate)
            .ok_or_else(|| Diagnostic::error("R002", "rate index is out of range").id(&inst.id))?;
        let (mut block, layout) = ty.build(&inst.params, &widths, rate).map_err(|d| d.id(&inst.id))?;
        let outputs: Vec<usize> = inst.outputs.iter().map(|s| s.len).collect();
        if layout.outputs != outputs || layout.continuous_states != inst.x.len || layout.discrete_states != inst.xd.len
        {
            return Err(Diagnostic::error("R002", "block layout does not match the compiled model").id(&inst.id));
        }
        block.initialize(&mut x[inst.x.range()], &mut xd[inst.xd.range()]);
        blocks.push((block, offsets(&widths), offsets(&outputs)));
    }

    let mut signals = vec![0.0; model.signals];
    let mut dx = vec![0.0; x.len()];
    let mut scratch = Vec::new();
    let mut traces: Traces = model.logged.keys().map(|k| (k.clone(), Vec::new())).collect();
    let steps = (t_end / dt).round() as usize;
    for step in 0..=steps {
        let t = step as f64 * dt;
        // All outputs first, then derivatives, so every block sees this step's inputs.
        for derivatives in [false, true] {
            for ((block, u_off, y_off), inst) in blocks.iter_mut().zip(&model.blocks) {
                // Gather inputs first, so outputs can borrow the buffer mutably.
                scratch.clear();
                inst.inputs
                    .iter()
                    .for_each(|s| scratch.extend_from_slice(&signals[s.range()]));
                let y = inst
                    .outputs
                    .first()
                    .map_or(0..0, |first| first.start..first.start + y_off.last().unwrap());
                let mut cx = Ctx {
                    t,
                    major: true,
                    u: &scratch,
                    u_off,
                    y: &mut signals[y],
                    y_off,
                    x: &x[inst.x.range()],
                    dx: &mut dx[inst.x.range()],
                    xd: &mut xd[inst.xd.range()],
                    z: &[],
                    r: &mut [],
                    zc: &mut [],
                };
                if derivatives {
                    block.derivatives(&mut cx);
                } else {
                    block.outputs(&mut cx);
                }
            }
        }
        for (name, slots) in &model.logged {
            traces.get_mut(name).unwrap().push(signals[slots.start]);
        }
        x.iter_mut().zip(&dx).for_each(|(x, dx)| *x += dt * dx);
    }
    Ok(traces)
}

fn offsets(widths: &[usize]) -> Vec<usize> {
    std::iter::once(0)
        .chain(widths.iter().scan(0, |acc, w| {
            *acc += w;
            Some(*acc)
        }))
        .collect()
}

// ---------------------------------------------------------------- Model API

/// In-memory Model API for one flat model: `block.add`, `block.remove`,
/// `connect`, `model.get` and `undo`. Every other method answers -32601.
/// IDs are sequential (`b_000000000001`), which real services must not do.
#[derive(Default)]
pub struct Service {
    blocks: BTreeMap<String, Json>,
    undo: Vec<BTreeMap<String, Json>>,
    next: u64,
}

impl Service {
    pub fn handle(&mut self, request: Request) -> Response {
        let outcome = match self.dispatch(request.command) {
            Ok(result) => Outcome::Result(result),
            Err(error) => Outcome::Error(error),
        };
        Response {
            jsonrpc: "2.0".into(),
            id: request.id,
            outcome,
        }
    }

    fn dispatch(&mut self, command: Command) -> Result<Json, RpcError> {
        let edit = |diff: Vec<Change>, created: Vec<String>| {
            Ok(json!(EditResult {
                diff,
                created,
                diagnostics: vec![]
            }))
        };
        match command {
            Command::BlockAdd(api::Edit { args, dry_run }) => {
                self.next += 1;
                let id = format!("b_{:012}", self.next);
                let block =
                    json!({"name": args.name, "type": args.type_name, "params": args.params, "inputs": args.inputs});
                let diff = vec![change(ChangeOp::Add, &id, None, Some(block.clone()))];
                if !dry_run {
                    self.snapshot();
                    self.blocks.insert(id.clone(), block);
                }
                edit(diff, vec![id])
            }
            Command::BlockRemove(api::Edit { args, dry_run }) => {
                let id = self.resolve(&args.target)?;
                let diff = vec![change(ChangeOp::Remove, &id, self.blocks.get(&id).cloned(), None)];
                if !dry_run {
                    self.snapshot();
                    self.blocks.remove(&id);
                }
                edit(diff, vec![])
            }
            Command::Connect(api::Edit { args, dry_run }) => {
                let (dest, port) = args
                    .destination
                    .split_once('.')
                    .ok_or_else(|| bad("destination must be `<block>.<port>`"))?;
                let (src, out) = args
                    .source
                    .split_once('.')
                    .ok_or_else(|| bad("source must be `<block>.<port>`"))?;
                let (dest, src) = (self.resolve(dest)?, self.resolve(src)?);
                let source = json!(format!("{src}.{out}"));
                let old = self.blocks[&dest]["inputs"].get(port).cloned();
                let diff = vec![change(
                    ChangeOp::Set,
                    &format!("{dest}.inputs.{port}"),
                    old,
                    Some(source.clone()),
                )];
                if !dry_run {
                    self.snapshot();
                    self.blocks.get_mut(&dest).unwrap()["inputs"][port] = source;
                }
                edit(diff, vec![])
            }
            Command::ModelGet { .. } => Ok(json!({ "blocks": self.blocks })),
            Command::Undo {} => {
                self.blocks = self.undo.pop().ok_or_else(|| bad("nothing to undo"))?;
                Ok(json!({}))
            }
            other => {
                let method = serde_json::to_value(&other)
                    .ok()
                    .and_then(|v| v["method"].as_str().map(String::from));
                Err(RpcError {
                    code: -32601,
                    message: format!("fake service does not implement {}", method.unwrap_or_default()),
                    diagnostics: vec![],
                })
            }
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(self.blocks.clone());
    }

    /// A name or an ID to an ID.
    fn resolve(&self, target: &str) -> Result<String, RpcError> {
        if self.blocks.contains_key(target) {
            return Ok(target.into());
        }
        self.blocks
            .iter()
            .find(|(_, b)| b["name"] == target)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| RpcError {
                code: 1,
                message: format!("no block `{target}`"),
                diagnostics: vec![],
            })
    }
}

fn change(op: ChangeOp, path: &str, old: Option<Json>, new: Option<Json>) -> Change {
    Change {
        op,
        file: "model.scmodel".into(),
        path: format!("blocks.{path}"),
        old,
        new,
    }
}

fn bad(message: &str) -> RpcError {
    RpcError {
        code: -32602,
        message: message.into(),
        diagnostics: vec![],
    }
}
