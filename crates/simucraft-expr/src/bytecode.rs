//! Scalar bytecode for a small register machine. Instruction `i` writes
//! register `i`, so a program needs no register allocation. Truth values are
//! 1.0 and 0.0. Both branches of `a if c else b` run; the expression language
//! has no side effects, so that is safe.

use simucraft_contracts::Diagnostic;

use crate::{BinOp, Expr, Node, arith, check_call, codes, compare, constant, math1};

#[derive(Clone, Copy, Debug)]
enum Op {
    Const(f64),
    Input(usize),
    Neg(usize),
    Not(usize),
    Bin(BinOp, usize, usize),
    Select(usize, usize, usize),
    F1(fn(f64) -> f64, usize),
    F2(fn(f64, f64) -> f64, usize, usize),
}

/// A compiled scalar expression.
#[derive(Clone, Debug)]
pub struct Program {
    ops: Vec<Op>,
}

/// Compile `e` to bytecode. `inputs` names the slots of the input vector that
/// [`Program::eval`] takes. Lists, indexing and signal functions are not scalar
/// and give a `C107` diagnostic.
pub fn compile(e: &Expr, inputs: &[&str]) -> Result<Program, Diagnostic> {
    let mut p = Program { ops: Vec::new() };
    p.emit(e, inputs)?;
    Ok(p)
}

impl Program {
    /// Evaluate with `x[i]` as the value of `inputs[i]`.
    pub fn eval(&self, x: &[f64]) -> f64 {
        // shortcut: allocates one register vector per call; pass in a buffer when the runtime needs it.
        let mut r: Vec<f64> = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            let v = match *op {
                Op::Const(v) => v,
                Op::Input(i) => x[i],
                Op::Neg(a) => -r[a],
                Op::Not(a) => f64::from(r[a] == 0.0),
                Op::Bin(BinOp::And, a, b) => f64::from(r[a] != 0.0 && r[b] != 0.0),
                Op::Bin(BinOp::Or, a, b) => f64::from(r[a] != 0.0 || r[b] != 0.0),
                Op::Bin(op, a, b) if op.is_comparison() => f64::from(compare(op, r[a], r[b])),
                Op::Bin(op, a, b) => arith(op, r[a], r[b]),
                Op::Select(c, a, b) => {
                    if r[c] != 0.0 {
                        r[a]
                    } else {
                        r[b]
                    }
                }
                Op::F1(f, a) => f(r[a]),
                Op::F2(f, a, b) => f(r[a], r[b]),
            };
            r.push(v);
        }
        r.last().copied().unwrap_or(f64::NAN)
    }

    fn push(&mut self, op: Op) -> usize {
        self.ops.push(op);
        self.ops.len() - 1
    }

    fn emit(&mut self, e: &Expr, inputs: &[&str]) -> Result<usize, Diagnostic> {
        let not_scalar = || e.error(codes::NOT_SCALAR, format!("`{e}` is not a scalar expression"));
        let op = match &e.node {
            Node::Num(v, unit) => Op::Const(v * unit.as_ref().map_or(1.0, |u| u.1.scale)),
            Node::Bool(b) => Op::Const(f64::from(*b)),
            Node::Name(n) => match (inputs.iter().position(|i| i == n), constant(n)) {
                (Some(i), _) => Op::Input(i),
                (None, Some(c)) => Op::Const(c),
                _ => return Err(e.error(codes::UNKNOWN_NAME, format!("unknown name `{n}`"))),
            },
            Node::Neg(a) => Op::Neg(self.emit(a, inputs)?),
            Node::Not(a) => Op::Not(self.emit(a, inputs)?),
            Node::Bin(op, a, b) => Op::Bin(*op, self.emit(a, inputs)?, self.emit(b, inputs)?),
            Node::If(then, cond, other) => {
                let c = self.emit(cond, inputs)?;
                Op::Select(c, self.emit(then, inputs)?, self.emit(other, inputs)?)
            }
            Node::Call(name, args, kwargs) => {
                check_call(e, name, args, kwargs)?;
                let regs: Vec<usize> = args.iter().map(|a| self.emit(a, inputs)).collect::<Result<_, _>>()?;
                match (name.as_str(), regs.as_slice()) {
                    (_, [a]) if math1(name).is_some() => Op::F1(math1(name).unwrap(), *a),
                    ("atan2", [a, b]) => Op::F2(f64::atan2, *a, *b),
                    ("min" | "max", [first, rest @ ..]) if !rest.is_empty() => {
                        let f = if name == "min" { f64::min } else { f64::max };
                        let mut acc = *first;
                        for r in &rest[..rest.len() - 1] {
                            acc = self.push(Op::F2(f, acc, *r));
                        }
                        Op::F2(f, acc, rest[rest.len() - 1])
                    }
                    ("clip", [x, lo, hi]) => {
                        let low = self.push(Op::F2(f64::max, *x, *lo));
                        Op::F2(f64::min, low, *hi)
                    }
                    _ => return Err(not_scalar()),
                }
            }
            Node::Index(..) | Node::List(_) => return Err(not_scalar()),
        };
        Ok(self.push(op))
    }
}
