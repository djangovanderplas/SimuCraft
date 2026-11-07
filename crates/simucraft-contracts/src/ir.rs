//! Compiled model: the flat form the compiler writes and the runtime runs.
//!
//! It is plain data (serde), so a runtime can run a hand-written compiled
//! model before the compiler exists. Wire form: JSON, `format = "simucraft.ir/0.1"`.
//!
//! Layout: one `f64` signal buffer; each output port owns a contiguous slot
//! range, and each input port reads one contiguous range (the compiler adds
//! copy blocks where a port gathers from several sources). A block's output
//! ports are adjacent, in port order. Blocks are listed
//! in execution order. A physical (acausal) network compiles to one block of
//! type `sys.Dae` whose parameters hold the residual bytecode.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::block::SampleTime;

pub const FORMAT: &str = "simucraft.ir/0.1";

/// A half-open slot range `[start, start + len)`, written `[start, len]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "[usize; 2]", into = "[usize; 2]")]
pub struct Slots {
    pub start: usize,
    pub len: usize,
}

impl Slots {
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }
    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.start + self.len
    }
}

impl From<[usize; 2]> for Slots {
    fn from([start, len]: [usize; 2]) -> Self {
        Self { start, len }
    }
}

impl From<Slots> for [usize; 2] {
    fn from(s: Slots) -> Self {
        [s.start, s.len]
    }
}

/// An evaluated parameter value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Scalar(f64),
    Bool(bool),
    Vector(Vec<f64>),
    Matrix { rows: usize, cols: usize, data: Vec<f64> },
    Text(String),
}

impl ParamValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParamValue::Scalar(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompiledModel {
    pub format: String,
    /// Size of each buffer.
    pub signals: usize,
    #[serde(default)]
    pub continuous_states: usize,
    #[serde(default)]
    pub discrete_states: usize,
    #[serde(default)]
    pub algebraic: usize,
    #[serde(default)]
    pub zero_crossings: usize,
    /// Distinct sample times; blocks refer to them by index.
    pub rates: Vec<SampleTime>,
    /// In execution order.
    pub blocks: Vec<BlockInstance>,
    /// Model inports and outports, and logged signals, by name.
    #[serde(default)]
    pub inports: BTreeMap<String, Slots>,
    #[serde(default)]
    pub outports: BTreeMap<String, Slots>,
    #[serde(default)]
    pub logged: BTreeMap<String, Slots>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockInstance {
    /// Path of stable IDs from the top model, `b_…/b_…`, for diagnostics.
    pub id: String,
    pub type_name: String,
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
    /// One range per input port, into the signal buffer.
    #[serde(default)]
    pub inputs: Vec<Slots>,
    #[serde(default)]
    pub outputs: Vec<Slots>,
    #[serde(default)]
    pub x: Slots,
    #[serde(default)]
    pub xd: Slots,
    #[serde(default)]
    pub z: Slots,
    #[serde(default)]
    pub zc: Slots,
    /// Index into `rates`.
    pub rate: usize,
}
