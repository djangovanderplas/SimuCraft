//! Block contract: what a block type declares, and the methods a runtime calls.
//!
//! Life cycle (the Simulink compile / link / loop split):
//! 1. The library describes the type: [`BlockType::spec`].
//! 2. The compiler evaluates parameters, sizes ports and calls
//!    [`BlockType::build`], which returns a [`Block`] and its [`Layout`].
//! 3. The runtime calls the [`Block`] methods in schedule order every step.
//!
//! All signal and state values are `f64` slots. Integer and boolean types keep
//! their meaning in the block (casting, wrapping), not in storage. Buses and
//! muxes are flattened by the compiler; each port is one contiguous slot range.

use serde::{Deserialize, Serialize};

use crate::Diagnostic;
use crate::ir::ParamValue;

/// Static description of a block type, used by the library browser, the
/// compiler and `library_docs`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockSpec {
    /// `library.BlockType`, for example `core.Gain`.
    pub type_name: String,
    pub doc: String,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    #[serde(default)]
    pub inputs: Vec<PortSpec>,
    /// More inputs after `inputs`, as many as a parameter sets, for example
    /// one per sign of `core.Sum`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variadic_inputs: Option<VariadicInputs>,
    #[serde(default)]
    pub outputs: Vec<PortSpec>,
    /// Physical (acausal) connectors.
    #[serde(default)]
    pub connectors: Vec<ConnectorSpec>,
}

/// Inputs whose number comes from a parameter. The ports are `<name>1`,
/// `<name>2`, and so on, where `<name>` is `port.name`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VariadicInputs {
    /// The name prefix, doc, type, width and unit of each port.
    pub port: PortSpec,
    /// The parameter that sets the number of ports: a whole number, or the
    /// length of a text or a vector.
    pub count: String,
}

impl BlockSpec {
    /// The input port names for these parameter values. If `params` has no
    /// value for the count parameter, its default counts: a number, or else
    /// the length of the default text.
    pub fn input_names(&self, params: &Params) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = self.inputs.iter().map(|p| p.name.clone()).collect();
        let Some(v) = &self.variadic_inputs else {
            return Ok(names);
        };
        let n = match params.get(&v.count) {
            Some(ParamValue::Text(t)) => t.chars().count(),
            Some(ParamValue::Vector(x)) => x.len(),
            Some(ParamValue::Scalar(x)) if *x >= 0.0 && x.fract() == 0.0 => *x as usize,
            Some(_) => return Err(format!("`{}` must be a whole number, a text or a vector", v.count)),
            None => {
                let default = self
                    .params
                    .iter()
                    .find(|p| p.name == v.count)
                    .and_then(|p| p.default.clone());
                let default = default.ok_or_else(|| format!("`{}` has no value and no default", v.count))?;
                default.parse::<usize>().unwrap_or_else(|_| default.chars().count())
            }
        };
        names.extend((1..=n).map(|i| format!("{}{i}", v.port.name)));
        Ok(names)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub doc: String,
    /// Expression used when the model gives no value.
    pub default: Option<String>,
    /// UCUM unit, `"1"` for dimensionless, `None` for unspecified.
    pub unit: Option<String>,
    /// Can change between runs without recompiling.
    #[serde(default)]
    pub tunable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortSpec {
    pub name: String,
    pub doc: String,
    #[serde(default)]
    pub dtype: DType,
    /// Number of slots; `None` means inherited from what it connects to.
    pub width: Option<usize>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectorSpec {
    pub name: String,
    pub doc: String,
    /// Physical domain: `elec`, `trans`, `rot`, `thermal`.
    pub domain: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    #[default]
    Inherit,
    F64,
    F32,
    I64,
    I32,
    I16,
    I8,
    U32,
    U16,
    U8,
    Bool,
}

/// When a block runs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SampleTime {
    /// Every minor and major step.
    Continuous,
    /// At `offset + k * period`.
    Discrete { period: f64, offset: f64 },
    /// Takes the rate of its inputs; resolved by the compiler.
    Inherited,
    /// Outputs never change after initialization.
    Constant,
}

/// What a built block needs from the runtime. The compiler assigns the slots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// Width of each input port, in `spec.inputs` order.
    pub inputs: Vec<usize>,
    /// Width of each output port, in `spec.outputs` order.
    pub outputs: Vec<usize>,
    /// For each input: does an output depend on it in the same step?
    /// Drives sorting and algebraic-loop detection.
    pub feedthrough: Vec<bool>,
    #[serde(default)]
    pub continuous_states: usize,
    #[serde(default)]
    pub discrete_states: usize,
    /// Algebraic variables, for blocks that are DAE residual systems.
    #[serde(default)]
    pub algebraic: usize,
    #[serde(default)]
    pub zero_crossings: usize,
    pub sample_time: SampleTime,
}

/// Evaluated parameters, by name, as the compiler hands them to `build`.
pub type Params = std::collections::BTreeMap<String, ParamValue>;

/// What `build` returns: the instance and the slots it needs.
pub type Built = (Box<dyn Block>, Layout);

/// A block type in a library.
pub trait BlockType: Send + Sync {
    fn spec(&self) -> &BlockSpec;

    /// Make one instance. The compiler resolves `input_widths`, in
    /// [`BlockSpec::input_names`] order, and `sample_time`: the block's `ts`
    /// field if the model gives one, else the rate it inherits from its
    /// sources. A block can declare another rate in its [`Layout`], but a
    /// `ts` field in the model always wins.
    fn build(&self, params: &Params, input_widths: &[usize], sample_time: SampleTime) -> Result<Built, Diagnostic>;
}

/// The runtime's view of one block for one step. Slices are this block's own
/// slots; port `i` is `u[u_off[i]..u_off[i + 1]]` (likewise for `y`).
pub struct Ctx<'a> {
    pub t: f64,
    /// True on a major time step: discrete updates and logging happen here.
    pub major: bool,
    pub u: &'a [f64],
    pub u_off: &'a [usize],
    pub y: &'a mut [f64],
    pub y_off: &'a [usize],
    /// Continuous states and their derivatives.
    pub x: &'a [f64],
    pub dx: &'a mut [f64],
    /// Discrete states.
    pub xd: &'a mut [f64],
    /// Algebraic variables and the residuals the block must drive to zero.
    pub z: &'a [f64],
    pub r: &'a mut [f64],
    /// Zero-crossing signals; the solver locates sign changes.
    pub zc: &'a mut [f64],
}

impl Ctx<'_> {
    pub fn u(&self, port: usize) -> &[f64] {
        &self.u[self.u_off[port]..self.u_off[port + 1]]
    }
    pub fn y(&mut self, port: usize) -> &mut [f64] {
        &mut self.y[self.y_off[port]..self.y_off[port + 1]]
    }
}

/// A built block instance. Only `outputs` is required.
pub trait Block: Send {
    /// Write initial continuous (`x0`) and discrete (`xd0`) states.
    fn initialize(&mut self, _x0: &mut [f64], _xd0: &mut [f64]) {}
    /// Compute `y` from `t`, `u` and states. Called every step, minor included.
    fn outputs(&mut self, cx: &mut Ctx);
    /// Advance discrete states on a sample hit (major steps only).
    fn update(&mut self, _cx: &mut Ctx) {}
    /// Write `dx/dt` into `dx`.
    fn derivatives(&mut self, _cx: &mut Ctx) {}
    /// Write DAE residuals into `r`.
    fn residuals(&mut self, _cx: &mut Ctx) {}
    /// Write zero-crossing signals into `zc`.
    fn zero_crossings(&mut self, _cx: &mut Ctx) {}
    /// Declared Jacobian of (`dx`, `y`) with respect to (`x`, `u`), if the block
    /// can give one. `None` means the runtime uses finite differences.
    fn jacobian(&self) -> Option<&dyn Jacobian> {
        None
    }
}

/// A block-declared Jacobian, as dense column-major blocks.
pub trait Jacobian {
    /// d(dx)/dx, `continuous_states` square.
    fn a(&self, cx: &Ctx, out: &mut [f64]);
    /// d(dx)/du, `continuous_states` x total input width.
    fn b(&self, cx: &Ctx, out: &mut [f64]);
    /// dy/dx, total output width x `continuous_states`.
    fn c(&self, cx: &Ctx, out: &mut [f64]);
    /// dy/du, total output width x total input width.
    fn d(&self, cx: &Ctx, out: &mut [f64]);
}
