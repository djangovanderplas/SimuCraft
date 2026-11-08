//! Tree-walking evaluator. Values are in coherent SI units: `2[ms]` is 0.002.

use simucraft_contracts::Diagnostic;

use crate::{BinOp, Expr, Node, arith, check_call, codes, compare, constant, math1};

/// A recorded signal: one value per time sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    /// Sample times, increasing.
    pub t: Vec<f64>,
    /// One value per sample time.
    pub y: Vec<f64>,
}

/// The value of an expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A number.
    Num(f64),
    /// A truth value.
    Bool(bool),
    /// An array; nested lists are matrices.
    List(Vec<Value>),
    /// A recorded signal, in metric expressions.
    Signal(Signal),
}

type Res = Result<Value, Diagnostic>;

/// Evaluate `e`. `value_of` gives the value of each name, `None` if unknown.
pub fn eval(e: &Expr, value_of: &dyn Fn(&str) -> Option<Value>) -> Res {
    let go = |e: &Expr| eval(e, value_of);
    let bad = |what: &str| e.error(codes::EVAL, format!("`{e}`: {what}"));
    match &e.node {
        Node::Num(v, unit) => Ok(Value::Num(v * unit.as_ref().map_or(1.0, |u| u.1.scale))),
        Node::Bool(b) => Ok(Value::Bool(*b)),
        Node::Name(n) => value_of(n)
            .or_else(|| constant(n).map(Value::Num))
            .ok_or_else(|| e.error(codes::UNKNOWN_NAME, format!("unknown name `{n}`"))),
        Node::Neg(a) => map1(&go(a)?, &|x| -x).ok_or_else(|| bad("cannot negate this value")),
        Node::Not(a) => Ok(Value::Bool(!truth(e, go(a)?)?)),
        Node::Bin(BinOp::And, a, b) => Ok(Value::Bool(truth(e, go(a)?)? && truth(e, go(b)?)?)),
        Node::Bin(BinOp::Or, a, b) => Ok(Value::Bool(truth(e, go(a)?)? || truth(e, go(b)?)?)),
        Node::Bin(op, a, b) if op.is_comparison() => match (go(a)?, go(b)?) {
            (Value::Num(x), Value::Num(y)) => Ok(Value::Bool(compare(*op, x, y))),
            (Value::Bool(x), Value::Bool(y)) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                Ok(Value::Bool((x == y) == (*op == BinOp::Eq)))
            }
            _ => Err(bad("comparisons need two numbers")),
        },
        Node::Bin(op, a, b) => {
            map2(&go(a)?, &go(b)?, &|x, y| arith(*op, x, y)).ok_or_else(|| bad("operands do not fit"))
        }
        Node::If(then, cond, other) => {
            if truth(e, go(cond)?)? {
                go(then)
            } else {
                go(other)
            }
        }
        Node::Index(a, i) => {
            let (Value::List(items), Value::Num(i)) = (go(a)?, go(i)?) else {
                return Err(bad("only a list can be indexed, with a number"));
            };
            let at = if i < 0.0 { items.len() as f64 + i } else { i };
            if at.fract() != 0.0 || at < 0.0 || at >= items.len() as f64 {
                return Err(bad("index out of range"));
            }
            Ok(items[at as usize].clone())
        }
        Node::List(items) => Ok(Value::List(items.iter().map(go).collect::<Result<_, _>>()?)),
        Node::Call(name, args, kwargs) => {
            check_call(e, name, args, kwargs)?;
            let args: Vec<Value> = args.iter().map(go).collect::<Result<_, _>>()?;
            call(name, &args, kwargs, value_of).map_err(|m| bad(&m))
        }
    }
}

fn truth(e: &Expr, v: Value) -> Result<bool, Diagnostic> {
    match v {
        Value::Bool(b) => Ok(b),
        _ => Err(e.error(codes::EVAL, format!("`{e}` needs True or False here"))),
    }
}

fn call(
    name: &str,
    args: &[Value],
    kwargs: &[(String, Expr)],
    value_of: &dyn Fn(&str) -> Option<Value>,
) -> Result<Value, String> {
    let fits = || "arguments do not fit".to_string();
    if let Some(f) = math1(name) {
        return map1(&args[0], &f).ok_or_else(fits);
    }
    let reduce = |f: &dyn Fn(&[f64]) -> f64| {
        let mut xs = Vec::new();
        flatten(&args[0], &mut xs).ok_or_else(fits)?;
        if xs.is_empty() {
            return Err("no values".to_string());
        }
        Ok(Value::Num(f(&xs)))
    };
    // `max(a)` reduces one array; `max(a, b, ...)` compares element by element.
    let fold = |f: fn(f64, f64) -> f64, init: f64| -> Result<Value, String> {
        if args.len() == 1 {
            return reduce(&|xs| xs.iter().copied().fold(init, f));
        }
        args[1..]
            .iter()
            .try_fold(args[0].clone(), |acc, v| map2(&acc, v, &f))
            .ok_or_else(fits)
    };
    match name {
        "atan2" => map2(&args[0], &args[1], &f64::atan2).ok_or_else(fits),
        "max" => fold(f64::max, f64::NEG_INFINITY),
        "min" => fold(f64::min, f64::INFINITY),
        "clip" => map2(&args[0], &args[1], &f64::max)
            .and_then(|v| map2(&v, &args[2], &f64::min))
            .ok_or_else(fits),
        "sum" => reduce(&|xs| xs.iter().sum()),
        "mean" => reduce(&|xs| xs.iter().sum::<f64>() / xs.len() as f64),
        "rms" => reduce(&|xs| (xs.iter().map(|x| x * x).sum::<f64>() / xs.len() as f64).sqrt()),
        "final" => reduce(&|xs| xs[xs.len() - 1]),
        "interp" => {
            let (mut xp, mut fp) = (Vec::new(), Vec::new());
            flatten(&args[1], &mut xp)
                .zip(flatten(&args[2], &mut fp))
                .ok_or_else(fits)?;
            if xp.len() != fp.len() || xp.is_empty() || xp.windows(2).any(|w| w[0] >= w[1]) {
                return Err("`interp(x, xp, fp)` needs increasing xp and fp of the same length".into());
            }
            map1(&args[0], &|x| interp(x, &xp, &fp)).ok_or_else(fits)
        }
        "settling_time" => {
            let Value::Signal(s) = &args[0] else {
                return Err("`settling_time` needs a signal".into());
            };
            let tol = match kwargs.first() {
                Some((_, e)) => match eval(e, value_of).map_err(|d| d.message)? {
                    Value::Num(t) => t,
                    _ => return Err("`tol` must be a number".into()),
                },
                None => 0.02,
            };
            Ok(Value::Num(settling_time(s, tol).ok_or("empty signal")?))
        }
        "der" => Err("`der` is only for acausal equations".into()),
        _ => unreachable!("check_call knows every function"),
    }
}

/// Linear interpolation, held constant outside `xp`.
fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let i = xp.partition_point(|&p| p <= x);
    if i == 0 {
        return fp[0];
    }
    if i == xp.len() {
        return fp[i - 1];
    }
    let w = (x - xp[i - 1]) / (xp[i] - xp[i - 1]);
    fp[i - 1] + w * (fp[i] - fp[i - 1])
}

/// First time after which the signal stays within `tol` (relative) of its final value.
fn settling_time(s: &Signal, tol: f64) -> Option<f64> {
    let last = *s.y.last()?;
    let band = tol * last.abs();
    let outside = s.y.iter().rposition(|y| (y - last).abs() > band);
    Some(match outside {
        Some(i) => s.t[(i + 1).min(s.t.len() - 1)],
        None => s.t[0],
    })
}

fn flatten(v: &Value, out: &mut Vec<f64>) -> Option<()> {
    match v {
        Value::Num(x) => out.push(*x),
        Value::List(items) => items.iter().try_for_each(|i| flatten(i, out))?,
        Value::Signal(s) => out.extend(&s.y),
        Value::Bool(_) => return None,
    }
    Some(())
}

/// Apply `f` to every number in `v`.
fn map1(v: &Value, f: &dyn Fn(f64) -> f64) -> Option<Value> {
    Some(match v {
        Value::Num(x) => Value::Num(f(*x)),
        Value::List(items) => Value::List(items.iter().map(|i| map1(i, f)).collect::<Option<_>>()?),
        Value::Signal(s) => Value::Signal(Signal {
            t: s.t.clone(),
            y: s.y.iter().map(|y| f(*y)).collect(),
        }),
        Value::Bool(_) => return None,
    })
}

/// Apply `f` element by element. A number broadcasts; lists and signals
/// must have the same length.
fn map2(a: &Value, b: &Value, f: &dyn Fn(f64, f64) -> f64) -> Option<Value> {
    use Value::*;
    Some(match (a, b) {
        (Num(x), Num(y)) => Num(f(*x, *y)),
        (Num(x), List(_) | Signal(_)) => map1(b, &|y| f(*x, y))?,
        (List(_) | Signal(_), Num(y)) => map1(a, &|x| f(x, *y))?,
        (List(xs), List(ys)) if xs.len() == ys.len() => {
            List(xs.iter().zip(ys).map(|(x, y)| map2(x, y, f)).collect::<Option<_>>()?)
        }
        (Signal(s), Signal(r)) if s.y.len() == r.y.len() => Signal(crate::Signal {
            t: s.t.clone(),
            y: s.y.iter().zip(&r.y).map(|(x, y)| f(*x, *y)).collect(),
        }),
        _ => return None,
    })
}
