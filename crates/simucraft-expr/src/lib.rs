//! The SimuCraft expression language (spec Section 9).
//!
//! - [`parse`] turns text into an [`Expr`].
//! - [`check_units`] checks and infers units without values.
//! - [`eval`] evaluates an expression in coherent SI units.
//! - [`derive`] gives the symbolic derivative with respect to one name.
//! - [`compile`] gives a [`Program`]: scalar bytecode for a small register machine.
//!
//! Evaluation always ends: the language has no loops, assignment, attribute
//! access or imports.

use std::fmt;

use simucraft_contracts::{Diagnostic, Span};

mod bytecode;
mod derive;
mod eval;
mod parse;
mod unit;
mod units;

pub use bytecode::{Program, compile};
pub use derive::derive;
pub use eval::{Signal, Value, eval};
pub use parse::parse;
pub use unit::Unit;
pub use units::check_units;

/// Diagnostic codes of this crate. The compiler owns `C001`-`C099`.
pub mod codes {
    /// The text is not a valid expression.
    pub const SYNTAX: &str = "C101";
    /// A name that the scope does not define.
    pub const UNKNOWN_NAME: &str = "C102";
    /// An unknown function, or wrong arguments for a function.
    pub const BAD_CALL: &str = "C103";
    /// Units that do not fit together.
    pub const UNIT: &str = "C104";
    /// A value of the wrong kind at evaluation (for example a list where a number must be).
    pub const EVAL: &str = "C105";
    /// A unit string that cannot be parsed.
    pub const BAD_UNIT: &str = "C106";
    /// A construct with no symbolic derivative or no scalar bytecode.
    pub const NOT_SCALAR: &str = "C107";
}

/// A parsed expression with its byte range in the source.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// What the expression is.
    pub node: Node,
    /// Byte range `[start, end)` in the source text.
    pub span: (usize, usize),
}

/// The kinds of expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A number as written, with its unit text and parsed unit if it has `[unit]`.
    Num(f64, Option<(String, Unit)>),
    /// `True` or `False`.
    Bool(bool),
    /// A name that the scope resolves.
    Name(String),
    /// `-a`.
    Neg(Box<Expr>),
    /// `not a`.
    Not(Box<Expr>),
    /// `a <op> b`.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// `then if cond else other`.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `f(args, key=value)`.
    Call(String, Vec<Expr>, Vec<(String, Expr)>),
    /// `a[i]`.
    Index(Box<Expr>, Box<Expr>),
    /// `[a, b, c]`.
    List(Vec<Expr>),
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`, with the sign of the divisor (as in Python)
    Rem,
    /// `**`
    Pow,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `and`
    And,
    /// `or`
    Or,
}

impl BinOp {
    fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Pow => "**",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            Eq => "==",
            Ne => "!=",
            And => "and",
            Or => "or",
        }
    }

    fn is_comparison(self) -> bool {
        use BinOp::*;
        matches!(self, Lt | Le | Gt | Ge | Eq | Ne)
    }
}

/// An error at a byte offset of a one-line expression.
fn error(code: &str, message: impl Into<String>, at: usize) -> Diagnostic {
    Diagnostic::error(code, message).span(Some(Span {
        line: 1,
        col: at as u32 + 1,
    }))
}

impl Expr {
    fn error(&self, code: &str, message: impl Into<String>) -> Diagnostic {
        error(code, message, self.span.0)
    }
}

/// Prints the expression back as text. Nested operators get parentheses.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nested = |e: &Expr| match e.node {
            Node::Bin(..) | Node::If(..) | Node::Neg(_) | Node::Not(_) => format!("({e})"),
            _ => e.to_string(),
        };
        let list = |items: &[Expr]| items.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
        match &self.node {
            Node::Num(v, None) => write!(f, "{v}"),
            Node::Num(v, Some((u, _))) => write!(f, "{v}[{u}]"),
            Node::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            Node::Name(n) => f.write_str(n),
            Node::Neg(a) => write!(f, "-{}", nested(a)),
            Node::Not(a) => write!(f, "not {}", nested(a)),
            Node::Bin(op, a, b) => write!(f, "{} {} {}", nested(a), op.symbol(), nested(b)),
            Node::If(t, c, o) => write!(f, "{} if {} else {}", nested(t), nested(c), nested(o)),
            Node::Call(name, args, kwargs) => {
                let kw = kwargs.iter().map(|(k, v)| format!(", {k}={v}")).collect::<String>();
                let kw = if args.is_empty() {
                    kw.trim_start_matches(", ").to_string()
                } else {
                    kw
                };
                write!(f, "{name}({}{kw})", list(args))
            }
            Node::Index(a, i) => write!(f, "{}[{i}]", nested(a)),
            Node::List(items) => write!(f, "[{}]", list(items)),
        }
    }
}

/// Number of positional arguments a fixed-library function takes, and its
/// keyword arguments. `None` if the function does not exist.
fn signature(name: &str) -> Option<(usize, usize, &'static [&'static str])> {
    Some(match name {
        "abs" | "sqrt" | "exp" | "log" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sign" | "sum" | "mean"
        | "rms" | "final" | "der" => (1, 1, &[]),
        "atan2" => (2, 2, &[]),
        "min" | "max" => (1, usize::MAX, &[]),
        "clip" | "interp" => (3, 3, &[]),
        "settling_time" => (1, 1, &["tol"]),
        _ => return None,
    })
}

/// Check a call against [`signature`].
fn check_call(e: &Expr, name: &str, args: &[Expr], kwargs: &[(String, Expr)]) -> Result<(), Diagnostic> {
    let (lo, hi, keys) =
        signature(name).ok_or_else(|| e.error(codes::BAD_CALL, format!("unknown function `{name}`")))?;
    if args.len() < lo || args.len() > hi {
        return Err(e.error(
            codes::BAD_CALL,
            format!("`{name}` takes {lo} to {hi} arguments, not {}", args.len()),
        ));
    }
    if let Some((k, _)) = kwargs.iter().find(|(k, _)| !keys.contains(&k.as_str())) {
        return Err(e.error(codes::BAD_CALL, format!("`{name}` has no argument `{k}`")));
    }
    Ok(())
}

/// One-argument scalar functions, shared by the evaluator and the bytecode.
fn math1(name: &str) -> Option<fn(f64) -> f64> {
    Some(match name {
        "abs" => f64::abs,
        "sqrt" => f64::sqrt,
        "exp" => f64::exp,
        "log" => f64::ln,
        "sin" => f64::sin,
        "cos" => f64::cos,
        "tan" => f64::tan,
        "asin" => f64::asin,
        "acos" => f64::acos,
        "atan" => f64::atan,
        "sign" => |x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                x
            }
        },
        _ => return None,
    })
}

/// Arithmetic on two scalars, shared by the evaluator and the bytecode.
fn arith(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a - b * (a / b).floor(),
        BinOp::Pow => a.powf(b),
        _ => unreachable!("not arithmetic"),
    }
}

/// Comparison of two scalars.
fn compare(op: BinOp, a: f64, b: f64) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Gt => a > b,
        BinOp::Ge => a >= b,
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        _ => unreachable!("not a comparison"),
    }
}

/// `pi` and `e`, seen when the scope does not define them.
fn constant(name: &str) -> Option<f64> {
    match name {
        "pi" => Some(std::f64::consts::PI),
        "e" => Some(std::f64::consts::E),
        _ => None,
    }
}
