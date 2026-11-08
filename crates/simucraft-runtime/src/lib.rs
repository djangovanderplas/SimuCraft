//! The SimuCraft runtime (spec Section 10, ADR 0001): it interprets a
//! [`CompiledModel`] block by block and integrates its continuous states with
//! a [`Solver`].
//!
//! A run is a sequence of major steps. Major steps fall on every output time
//! and on every sample hit of a discrete rate. On a major step the runtime
//! computes all outputs, logs, then updates discrete states. Between major
//! steps the solver calls the model at minor steps: discrete blocks hold their
//! outputs there.

use std::collections::BTreeMap;
use std::ops::Range;

use simucraft_contracts::Diagnostic;
use simucraft_contracts::block::{Block, BlockType, Ctx, SampleTime};
use simucraft_contracts::ir::{CompiledModel, Slots};

mod bdf;
mod solver;

pub use bdf::Bdf;
pub use solver::{DormandPrince, Euler, Rhs, Rk4, Solver};

/// Diagnostic codes of the runtime.
pub mod codes {
    /// The library has no block type of this name.
    pub const UNKNOWN_TYPE: &str = "R001";
    /// The block's layout does not match its slots in the compiled model.
    pub const LAYOUT: &str = "R002";
    /// The variable-step solver cannot make progress.
    pub const STEP: &str = "R003";
}

/// Logged signals of a run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traces {
    /// Output times.
    pub t: Vec<f64>,
    /// Signal name -> values. A signal of width `w` has `w` values per output
    /// time, one row after the other.
    pub signals: BTreeMap<String, Vec<f64>>,
}

struct Inst {
    block: Box<dyn Block>,
    inputs: Vec<Slots>,
    u_off: Vec<usize>,
    y: Range<usize>,
    y_off: Vec<usize>,
    x: Range<usize>,
    xd: Range<usize>,
    z: Range<usize>,
    zc: Range<usize>,
    rate: SampleTime,
}

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Outputs,
    Update,
    Derivatives,
}

/// A compiled model, built and ready to run.
pub struct Simulation {
    blocks: Vec<Inst>,
    signals: Vec<f64>,
    x0: Vec<f64>,
    xd: Vec<f64>,
    z: Vec<f64>,
    r: Vec<f64>,
    zc: Vec<f64>,
    scratch: Vec<f64>,
    rates: Vec<SampleTime>,
    logged: BTreeMap<String, Slots>,
}

impl Simulation {
    /// Build every block from `library` and set the initial states. Each
    /// block's layout must match the slots the compiled model gives it.
    pub fn new(model: &CompiledModel, library: &[Box<dyn BlockType>]) -> Result<Self, Diagnostic> {
        let mut x0 = vec![0.0; model.continuous_states];
        let mut xd = vec![0.0; model.discrete_states];
        let mut blocks = Vec::with_capacity(model.blocks.len());
        for inst in &model.blocks {
            let ty = library
                .iter()
                .find(|t| t.spec().type_name == inst.type_name)
                .ok_or_else(|| {
                    Diagnostic::error(codes::UNKNOWN_TYPE, format!("unknown block type `{}`", inst.type_name))
                        .id(&inst.id)
                })?;
            let widths: Vec<usize> = inst.inputs.iter().map(|s| s.len).collect();
            let outputs: Vec<usize> = inst.outputs.iter().map(|s| s.len).collect();
            let rate = *model.rates.get(inst.rate).ok_or_else(|| {
                Diagnostic::error(codes::LAYOUT, format!("rate index {} is out of range", inst.rate)).id(&inst.id)
            })?;
            let (mut block, layout) = ty.build(&inst.params, &widths, rate).map_err(|d| d.id(&inst.id))?;
            let adjacent = inst.outputs.windows(2).all(|w| w[0].start + w[0].len == w[1].start);
            if layout.outputs != outputs
                || !adjacent
                || layout.continuous_states != inst.x.len
                || layout.discrete_states != inst.xd.len
                || layout.algebraic != inst.z.len
                || layout.zero_crossings != inst.zc.len
            {
                return Err(
                    Diagnostic::error(codes::LAYOUT, "block layout does not match the compiled model").id(&inst.id),
                );
            }
            block.initialize(&mut x0[inst.x.range()], &mut xd[inst.xd.range()]);
            let start = inst.outputs.first().map_or(0, |s| s.start);
            blocks.push(Inst {
                block,
                inputs: inst.inputs.clone(),
                u_off: offsets(&widths),
                y: start..start + outputs.iter().sum::<usize>(),
                y_off: offsets(&outputs),
                x: inst.x.range(),
                xd: inst.xd.range(),
                z: inst.z.range(),
                zc: inst.zc.range(),
                rate,
            });
        }
        Ok(Self {
            blocks,
            signals: vec![0.0; model.signals],
            x0,
            xd,
            z: vec![0.0; model.algebraic],
            r: vec![0.0; model.algebraic],
            zc: vec![0.0; model.zero_crossings],
            scratch: Vec::new(),
            rates: model.rates.clone(),
            logged: model.logged.clone(),
        })
    }

    /// Run from t = 0 to `t_end` and collect the logged signals every `output_step`.
    pub fn run(&mut self, solver: &mut dyn Solver, t_end: f64, output_step: f64) -> Result<Traces, Diagnostic> {
        let logged = self.logged.clone();
        let mut traces = Traces {
            t: Vec::new(),
            signals: logged.keys().map(|k| (k.clone(), Vec::new())).collect(),
        };
        self.stream(solver, t_end, output_step, &mut |t, signals| {
            traces.t.push(t);
            for (name, slots) in &logged {
                traces
                    .signals
                    .get_mut(name)
                    .expect("one trace per logged signal")
                    .extend(&signals[slots.range()]);
            }
        })?;
        Ok(traces)
    }

    /// Run from t = 0 to `t_end` and call `log` with the signal buffer at
    /// every output time, so a caller can stream results.
    pub fn stream(
        &mut self,
        solver: &mut dyn Solver,
        t_end: f64,
        output_step: f64,
        log: &mut dyn FnMut(f64, &[f64]),
    ) -> Result<(), Diagnostic> {
        let mut x = self.x0.clone();
        let mut dx = vec![0.0; x.len()];
        let eps = 1e-9 * output_step;
        let mut t = 0.0;
        let mut k = 0u64; // index of the next output time
        loop {
            self.pass(Method::Outputs, t, true, &x, &mut dx);
            if (t - k as f64 * output_step).abs() <= eps {
                log(t, &self.signals);
                k += 1;
            }
            self.pass(Method::Update, t, true, &x, &mut dx);
            if t >= t_end - eps {
                return Ok(());
            }
            let next = self.next_hit(t).min(k as f64 * output_step).min(t_end);
            if !x.is_empty() {
                let mut rhs = |t: f64, x: &[f64], dx: &mut [f64]| {
                    self.pass(Method::Outputs, t, false, x, dx);
                    self.pass(Method::Derivatives, t, false, x, dx);
                };
                solver.advance(&mut rhs, t, &mut x, next)?;
            }
            t = next;
        }
    }

    /// The first sample hit of any discrete rate after `t`.
    fn next_hit(&self, t: f64) -> f64 {
        self.rates
            .iter()
            .filter_map(|r| match *r {
                SampleTime::Discrete { period, offset } => {
                    let n = ((t - offset) / period + 1e-9).floor().max(-1.0) + 1.0;
                    Some(offset + n * period)
                }
                _ => None,
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Call `method` on every block that runs at `t`.
    fn pass(&mut self, method: Method, t: f64, major: bool, x: &[f64], dx: &mut [f64]) {
        let Self {
            blocks,
            signals,
            xd,
            z,
            r,
            zc,
            scratch,
            ..
        } = self;
        for b in blocks.iter_mut() {
            let runs = match b.rate {
                SampleTime::Discrete { period, offset } => {
                    major && method != Method::Derivatives && hit(t, period, offset)
                }
                SampleTime::Constant => method == Method::Outputs,
                SampleTime::Continuous | SampleTime::Inherited => method != Method::Update || major,
            };
            if !runs {
                continue;
            }
            // Gather inputs first, so outputs can borrow the buffer mutably.
            scratch.clear();
            b.inputs
                .iter()
                .for_each(|s| scratch.extend_from_slice(&signals[s.range()]));
            let mut cx = Ctx {
                t,
                major,
                u: scratch,
                u_off: &b.u_off,
                y: &mut signals[b.y.clone()],
                y_off: &b.y_off,
                x: &x[b.x.clone()],
                dx: &mut dx[b.x.clone()],
                xd: &mut xd[b.xd.clone()],
                z: &z[b.z.clone()],
                r: &mut r[b.z.clone()],
                zc: &mut zc[b.zc.clone()],
            };
            match method {
                Method::Outputs => b.block.outputs(&mut cx),
                Method::Update => b.block.update(&mut cx),
                Method::Derivatives => b.block.derivatives(&mut cx),
            }
        }
    }
}

fn hit(t: f64, period: f64, offset: f64) -> bool {
    let n = (t - offset) / period;
    n > -1e-9 && (n - n.round()).abs() < 1e-9
}

fn offsets(widths: &[usize]) -> Vec<usize> {
    std::iter::once(0)
        .chain(widths.iter().scan(0, |acc, w| {
            *acc += w;
            Some(*acc)
        }))
        .collect()
}
