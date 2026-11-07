//! The core block library (`core.*`), spec requirement M10 (first part).
//!
//! Each block works element by element on its input width, unless its doc
//! says otherwise. Parameters a model leaves out take the default in the
//! block's [`BlockSpec`]. The compiler passes a block's `ts` field as the
//! parameter `ts`; blocks that can be discrete read it.

use simucraft_contracts::Diagnostic;
use simucraft_contracts::block::{
    Block, BlockSpec, BlockType, Built, Ctx, DType, Layout, ParamSpec, Params, PortSpec, SampleTime,
};
use simucraft_contracts::ir::ParamValue;

/// Diagnostic codes of this library. They come back from the compiler's build step.
pub mod codes {
    /// A parameter has the wrong kind or an invalid value.
    pub const PARAM: &str = "C201";
    /// Input widths or port counts do not fit the parameters.
    pub const WIDTH: &str = "C202";
}

/// All block types of the core library.
pub fn library() -> Vec<Box<dyn BlockType>> {
    vec![
        def(
            "core.Constant",
            "Outputs a constant value.",
            &[],
            &["y"],
            &[("value", "1", "Output value")],
            |p, w| {
                let v = p.num("value")?;
                Ok((Box::new(Constant(v)), p.layout(w, 1, &[], SampleTime::Constant)))
            },
        ),
        def(
            "core.Step",
            "Outputs `initial` before `time` and `final` from `time` on.",
            &[],
            &["y"],
            &[
                ("time", "1", "Step time [s]"),
                ("initial", "0", "Value before the step"),
                ("final", "1", "Value after the step"),
            ],
            |p, w| {
                let s = Step {
                    time: p.num("time")?,
                    initial: p.num("initial")?,
                    last: p.num("final")?,
                };
                Ok((Box::new(s), p.layout(w, 1, &[], SampleTime::Continuous)))
            },
        ),
        def(
            "core.Gain",
            "y = k * u.",
            &["u"],
            &["y"],
            &[("k", "1", "Gain")],
            |p, w| {
                let k = p.num("k")?;
                Ok((Box::new(Gain(k)), p.layout(w, w[0], &[true], SampleTime::Inherited)))
            },
        ),
        def(
            "core.Sum",
            "Adds or subtracts its inputs, one input per sign: `u1`, `u2`, and so on.",
            &["u1", "u2"],
            &["y"],
            &[("signs", "++", "One `+` or `-` per input")],
            |p, w| {
                let signs: Vec<f64> = p
                    .text("signs")?
                    .chars()
                    .map(|c| if c == '-' { -1.0 } else { 1.0 })
                    .collect();
                if signs.len() != w.len() {
                    return Err(Diagnostic::error(
                        codes::WIDTH,
                        format!("`signs` has {} signs for {} inputs", signs.len(), w.len()),
                    ));
                }
                let width = same_width(w)?;
                Ok((
                    Box::new(Sum(signs)),
                    p.layout(w, width, &vec![true; w.len()], SampleTime::Inherited),
                ))
            },
        ),
        def(
            "core.Integrator",
            "y = x, dx/dt = u.",
            &["u"],
            &["y"],
            &[("x0", "0", "Initial state")],
            |p, w| {
                let x0 = p.num("x0")?;
                let mut l = p.layout(w, w[0], &[false], SampleTime::Continuous);
                l.continuous_states = w[0];
                Ok((Box::new(Integrator(x0)), l))
            },
        ),
        def(
            "core.TransferFcn",
            "Continuous transfer function num(s) / den(s), coefficients in falling powers of s. Scalar input.",
            &["u"],
            &["y"],
            &[
                ("num", "[1]", "Numerator coefficients"),
                ("den", "[1, 1]", "Denominator coefficients"),
            ],
            |p, w| {
                let tf = TransferFcn::new(&p.vector("num")?, &p.vector("den")?)?;
                scalar(w)?;
                let mut l = p.layout(w, 1, &[tf.d != 0.0], SampleTime::Continuous);
                l.continuous_states = tf.a.len();
                Ok((Box::new(tf), l))
            },
        ),
        def(
            "core.PID",
            "PID controller P + I/s + D N s/(s + N). With `ts` > 0 it is discrete, with forward-Euler integration.",
            &["u"],
            &["y"],
            &[
                ("P", "1", "Proportional gain"),
                ("I", "0", "Integral gain"),
                ("D", "0", "Derivative gain"),
                ("N", "100", "Derivative filter coefficient"),
                ("ts", "0", "Sample time [s]; 0 is continuous"),
            ],
            |p, w| {
                let pid = Pid {
                    p: p.num("P")?,
                    i: p.num("I")?,
                    d: p.num("D")?,
                    n: p.num("N")?,
                    ts: p.num("ts")?,
                };
                let width = w[0];
                let rate = if pid.ts > 0.0 {
                    SampleTime::Discrete {
                        period: pid.ts,
                        offset: 0.0,
                    }
                } else {
                    SampleTime::Continuous
                };
                let mut l = p.layout(w, width, &[true], rate);
                // Per element: the integral and the derivative filter state.
                if pid.ts > 0.0 {
                    l.discrete_states = 2 * width;
                } else {
                    l.continuous_states = 2 * width;
                }
                Ok((Box::new(pid), l))
            },
        ),
        def(
            "core.Saturation",
            "Limits u to [lower, upper].",
            &["u"],
            &["y"],
            &[("upper", "0.5", "Upper limit"), ("lower", "-0.5", "Lower limit")],
            |p, w| {
                let (lower, upper) = (p.num("lower")?, p.num("upper")?);
                if lower > upper {
                    return Err(Diagnostic::error(codes::PARAM, "`lower` must not be above `upper`"));
                }
                Ok((
                    Box::new(Saturation(lower, upper)),
                    p.layout(w, w[0], &[true], SampleTime::Inherited),
                ))
            },
        ),
        def(
            "core.Scope",
            "Shows its input in the editor. The compiler logs the input.",
            &["u"],
            &[],
            &[],
            |p, w| Ok((Box::new(Nothing), p.layout(w, 0, &[false], SampleTime::Inherited))),
        ),
        def(
            "core.Inport",
            "A model input. The runtime writes its value; in a subsystem the compiler replaces it.",
            &[],
            &["y"],
            &[("width", "1", "Number of elements")],
            |p, w| {
                let width = p.num("width")?;
                if width < 1.0 || width.fract() != 0.0 {
                    return Err(Diagnostic::error(
                        codes::PARAM,
                        "`width` must be a whole number of 1 or more",
                    ));
                }
                Ok((
                    Box::new(Nothing),
                    p.layout(w, width as usize, &[], SampleTime::Inherited),
                ))
            },
        ),
        def(
            "core.Outport",
            "A model output. The compiler maps it to the slots of its input.",
            &["u"],
            &[],
            &[],
            |p, w| Ok((Box::new(Nothing), p.layout(w, 0, &[false], SampleTime::Inherited))),
        ),
    ]
}

// ---------------------------------------------------------------- definitions

type BuildFn = fn(&Args, &[usize]) -> Result<Built, Diagnostic>;

struct Def {
    spec: BlockSpec,
    build: BuildFn,
}

impl BlockType for Def {
    fn spec(&self) -> &BlockSpec {
        &self.spec
    }
    fn build(&self, params: &Params, input_widths: &[usize]) -> Result<Built, Diagnostic> {
        if input_widths.contains(&0) {
            return Err(Diagnostic::error(codes::WIDTH, "an input has width 0"));
        }
        // Only `core.Sum` takes one input per sign; all others take the ports of their spec.
        let variadic = self.spec.type_name == "core.Sum" && !input_widths.is_empty();
        if !variadic && input_widths.len() != self.spec.inputs.len() {
            return Err(Diagnostic::error(
                codes::WIDTH,
                format!(
                    "`{}` takes {} inputs, not {}",
                    self.spec.type_name,
                    self.spec.inputs.len(),
                    input_widths.len()
                ),
            ));
        }
        (self.build)(
            &Args {
                spec: &self.spec,
                params,
            },
            input_widths,
        )
    }
}

fn def(
    type_name: &str,
    doc: &str,
    inputs: &[&str],
    outputs: &[&str],
    params: &[(&str, &str, &str)],
    build: BuildFn,
) -> Box<dyn BlockType> {
    let port = |name: &&str| PortSpec {
        name: name.to_string(),
        doc: String::new(),
        dtype: DType::F64,
        width: None,
        unit: None,
    };
    Box::new(Def {
        spec: BlockSpec {
            type_name: type_name.into(),
            doc: doc.into(),
            params: params
                .iter()
                .map(|(name, default, doc)| ParamSpec {
                    name: name.to_string(),
                    doc: doc.to_string(),
                    default: Some(default.to_string()),
                    unit: None,
                    tunable: true,
                })
                .collect(),
            inputs: inputs.iter().map(port).collect(),
            outputs: outputs.iter().map(port).collect(),
            connectors: vec![],
        },
        build,
    })
}

/// Parameters with the spec's defaults behind them.
struct Args<'a> {
    spec: &'a BlockSpec,
    params: &'a Params,
}

impl Args<'_> {
    /// The given value, or the spec default read as a literal.
    fn get(&self, name: &str) -> ParamValue {
        if let Some(v) = self.params.get(name) {
            return v.clone();
        }
        let default = self
            .spec
            .params
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.default.as_deref());
        let default = default.expect("every parameter a block reads has a default");
        if let Ok(v) = default.parse() {
            return ParamValue::Scalar(v);
        }
        match default.strip_prefix('[').and_then(|d| d.strip_suffix(']')) {
            Some(items) => ParamValue::Vector(
                items
                    .split(',')
                    .map(|s| s.trim().parse().expect("numeric default"))
                    .collect(),
            ),
            None => ParamValue::Text(default.into()),
        }
    }

    fn bad(&self, name: &str, want: &str) -> Diagnostic {
        Diagnostic::error(
            codes::PARAM,
            format!("`{}`: parameter `{name}` must be {want}", self.spec.type_name),
        )
    }

    fn num(&self, name: &str) -> Result<f64, Diagnostic> {
        match self.get(name) {
            ParamValue::Scalar(v) if v.is_finite() => Ok(v),
            _ => Err(self.bad(name, "a finite number")),
        }
    }

    fn text(&self, name: &str) -> Result<String, Diagnostic> {
        match self.get(name) {
            ParamValue::Text(s) => Ok(s),
            _ => Err(self.bad(name, "a string")),
        }
    }

    fn vector(&self, name: &str) -> Result<Vec<f64>, Diagnostic> {
        match self.get(name) {
            ParamValue::Vector(v) => Ok(v),
            ParamValue::Scalar(v) => Ok(vec![v]),
            _ => Err(self.bad(name, "a number or a vector")),
        }
    }

    /// One output port of `width` (none if 0), no states.
    fn layout(&self, inputs: &[usize], width: usize, feedthrough: &[bool], sample_time: SampleTime) -> Layout {
        Layout {
            inputs: inputs.to_vec(),
            outputs: if self.spec.outputs.is_empty() {
                vec![]
            } else {
                vec![width]
            },
            feedthrough: feedthrough.to_vec(),
            continuous_states: 0,
            discrete_states: 0,
            algebraic: 0,
            zero_crossings: 0,
            sample_time,
        }
    }
}

/// Inputs of width 1 broadcast; all others must agree.
fn same_width(w: &[usize]) -> Result<usize, Diagnostic> {
    let width = w.iter().copied().max().unwrap_or(1);
    if w.iter().any(|&x| x != 1 && x != width) {
        return Err(Diagnostic::error(
            codes::WIDTH,
            format!("input widths {w:?} do not fit"),
        ));
    }
    Ok(width)
}

fn scalar(w: &[usize]) -> Result<(), Diagnostic> {
    match w {
        [1] => Ok(()),
        _ => Err(Diagnostic::error(codes::WIDTH, "this block takes a scalar input")),
    }
}

// ---------------------------------------------------------------- blocks

struct Nothing;
impl Block for Nothing {
    fn outputs(&mut self, _: &mut Ctx) {}
}

struct Constant(f64);
impl Block for Constant {
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y.fill(self.0);
    }
}

struct Step {
    time: f64,
    initial: f64,
    last: f64,
}
impl Block for Step {
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y.fill(if cx.t >= self.time { self.last } else { self.initial });
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

struct Sum(Vec<f64>);
impl Block for Sum {
    fn outputs(&mut self, cx: &mut Ctx) {
        let width = cx.y.len();
        cx.y.fill(0.0);
        for (port, sign) in self.0.iter().enumerate() {
            let u = &cx.u[cx.u_off[port]..cx.u_off[port + 1]];
            for i in 0..width {
                cx.y[i] += sign * u[if u.len() == 1 { 0 } else { i }];
            }
        }
    }
}

struct Integrator(f64);
impl Block for Integrator {
    fn initialize(&mut self, x0: &mut [f64], _: &mut [f64]) {
        x0.fill(self.0);
    }
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y.copy_from_slice(cx.x);
    }
    fn derivatives(&mut self, cx: &mut Ctx) {
        cx.dx.copy_from_slice(cx.u);
    }
}

/// Controllable canonical form: `x' = A x + b u`, `y = c x + d u`. With
/// `a` the normalized denominator, `x[n-1]' = u - sum(a[k] x[n-k])`.
struct TransferFcn {
    a: Vec<f64>,
    c: Vec<f64>,
    d: f64,
}

impl TransferFcn {
    fn new(num: &[f64], den: &[f64]) -> Result<Self, Diagnostic> {
        let bad = |m: &str| Diagnostic::error(codes::PARAM, format!("`core.TransferFcn`: {m}"));
        let lead = *den.first().ok_or_else(|| bad("`den` is empty"))?;
        if lead == 0.0 || den.iter().chain(num).any(|v| !v.is_finite()) {
            return Err(bad(
                "`den` must start with a nonzero number, and all coefficients must be finite",
            ));
        }
        if num.len() > den.len() {
            return Err(bad(
                "`num` has a higher order than `den`; the transfer function is not proper",
            ));
        }
        let n = den.len() - 1;
        let a: Vec<f64> = den[1..].iter().map(|v| v / lead).collect();
        let mut p = vec![0.0; den.len() - num.len()];
        p.extend(num.iter().map(|v| v / lead));
        let d = p[0];
        // c[n - k] = p[k] - p[0] a[k], for k = 1..n
        let mut c = vec![0.0; n];
        for k in 1..=n {
            c[n - k] = p[k] - d * a[k - 1];
        }
        Ok(Self { a, c, d })
    }
}

impl Block for TransferFcn {
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y[0] = self.d * cx.u[0] + self.c.iter().zip(cx.x).map(|(c, x)| c * x).sum::<f64>();
    }
    fn derivatives(&mut self, cx: &mut Ctx) {
        let n = self.a.len();
        for j in 0..n.saturating_sub(1) {
            cx.dx[j] = cx.x[j + 1];
        }
        if n > 0 {
            cx.dx[n - 1] = cx.u[0] - (1..=n).map(|k| self.a[k - 1] * cx.x[n - k]).sum::<f64>();
        }
    }
}

/// States per element `i`: integral at `i`, derivative filter at `width + i`.
struct Pid {
    p: f64,
    i: f64,
    d: f64,
    n: f64,
    ts: f64,
}

impl Block for Pid {
    fn outputs(&mut self, cx: &mut Ctx) {
        let width = cx.y.len();
        let s: &[f64] = if self.ts > 0.0 { cx.xd } else { cx.x };
        for k in 0..width {
            let e = cx.u[k];
            cx.y[k] = self.p * e + self.i * s[k] + self.d * self.n * (e - s[width + k]);
        }
    }
    fn derivatives(&mut self, cx: &mut Ctx) {
        let width = cx.u.len();
        for k in 0..width {
            cx.dx[k] = cx.u[k];
            cx.dx[width + k] = self.n * (cx.u[k] - cx.x[width + k]);
        }
    }
    fn update(&mut self, cx: &mut Ctx) {
        if self.ts <= 0.0 {
            return;
        }
        let width = cx.u.len();
        for k in 0..width {
            cx.xd[k] += self.ts * cx.u[k];
            cx.xd[width + k] += self.ts * self.n * (cx.u[k] - cx.xd[width + k]);
        }
    }
}

struct Saturation(f64, f64);
impl Block for Saturation {
    fn outputs(&mut self, cx: &mut Ctx) {
        for (y, u) in cx.y.iter_mut().zip(cx.u) {
            *y = u.clamp(self.0, self.1);
        }
    }
}
