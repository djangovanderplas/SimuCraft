//! Symbolic derivatives. The result is an ordinary [`Expr`]; the builders
//! below fold zeros and ones so results stay small.

use simucraft_contracts::Diagnostic;

use crate::{BinOp, Expr, Node, codes};

/// The derivative of `e` with respect to the name `var`. Piecewise functions
/// (`abs`, `min`, `max`, `clip`, `if`) give the derivative of the active piece.
pub fn derive(e: &Expr, var: &str) -> Result<Expr, Diagnostic> {
    let d = |x: &Expr| derive(x, var);
    let span = e.span;
    let at = |node| Expr { node, span };
    let num = |v| num(v, span);
    let not_smooth = || e.error(codes::NOT_SCALAR, format!("`{e}` has no symbolic derivative"));
    Ok(match &e.node {
        Node::Num(..) | Node::Bool(_) => num(0.0),
        Node::Name(n) => num(if n == var { 1.0 } else { 0.0 }),
        Node::Neg(a) => neg(d(a)?),
        Node::Not(_) => num(0.0),
        Node::Bin(op, a, b) => {
            let (a, b) = (a.as_ref().clone(), b.as_ref().clone());
            match op {
                BinOp::Add => add(d(&a)?, d(&b)?),
                BinOp::Sub => sub(d(&a)?, d(&b)?),
                BinOp::Mul => add(mul(d(&a)?, b.clone()), mul(a.clone(), d(&b)?)),
                // (a/b)' = a'/b - a b' / b^2
                BinOp::Div => sub(div(d(&a)?, b.clone()), div(mul(a.clone(), d(&b)?), mul(b.clone(), b))),
                BinOp::Rem if is_zero(&d(&b)?) => d(&a)?,
                BinOp::Rem => return Err(not_smooth()),
                BinOp::Pow => {
                    let (da, db) = (d(&a)?, d(&b)?);
                    if is_zero(&db) {
                        // (a^n)' = n a^(n-1) a'
                        mul(mul(b.clone(), pow(a, sub(b, num(1.0)))), da)
                    } else {
                        // (a^b)' = a^b (b' ln a + b a' / a)
                        let ln = call("log", vec![a.clone()], span);
                        mul(at(e.node.clone()), add(mul(db, ln), div(mul(b, da), a)))
                    }
                }
                _ => num(0.0), // comparisons and logic are piecewise constant
            }
        }
        Node::If(then, cond, other) => at(Node::If(Box::new(d(then)?), cond.clone(), Box::new(d(other)?))),
        Node::Call(name, args, _) => {
            let u = || args[0].clone();
            let du = || d(&args[0]);
            let f = |name: &str, x: Expr| call(name, vec![x], span);
            match (name.as_str(), args.len()) {
                ("abs", 1) => mul(f("sign", u()), du()?),
                ("sign", 1) => num(0.0),
                ("sqrt", 1) => div(du()?, mul(num(2.0), f("sqrt", u()))),
                ("exp", 1) => mul(f("exp", u()), du()?),
                ("log", 1) => div(du()?, u()),
                ("sin", 1) => mul(f("cos", u()), du()?),
                ("cos", 1) => neg(mul(f("sin", u()), du()?)),
                ("tan", 1) => div(du()?, pow(f("cos", u()), num(2.0))),
                ("asin", 1) => div(du()?, f("sqrt", sub(num(1.0), mul(u(), u())))),
                ("acos", 1) => neg(div(du()?, f("sqrt", sub(num(1.0), mul(u(), u()))))),
                ("atan", 1) => div(du()?, add(num(1.0), mul(u(), u()))),
                ("atan2", 2) => {
                    let (y, x) = (args[0].clone(), args[1].clone());
                    let top = sub(mul(x.clone(), d(&y)?), mul(y.clone(), d(&x)?));
                    div(top, add(mul(x.clone(), x), mul(y.clone(), y)))
                }
                ("min" | "max", 2) => {
                    let op = if name == "min" { BinOp::Le } else { BinOp::Ge };
                    let cond = bin(op, args[0].clone(), args[1].clone());
                    at(Node::If(Box::new(du()?), Box::new(cond), Box::new(d(&args[1])?)))
                }
                ("clip", 3) => {
                    let (x, lo, hi) = (&args[0], &args[1], &args[2]);
                    let inner = at(Node::If(
                        Box::new(d(hi)?),
                        Box::new(bin(BinOp::Gt, x.clone(), hi.clone())),
                        Box::new(d(x)?),
                    ));
                    let below = bin(BinOp::Lt, x.clone(), lo.clone());
                    at(Node::If(Box::new(d(lo)?), Box::new(below), Box::new(inner)))
                }
                _ => return Err(not_smooth()),
            }
        }
        Node::Index(..) | Node::List(_) => return Err(not_smooth()),
    })
}

fn num(v: f64, span: (usize, usize)) -> Expr {
    Expr {
        node: Node::Num(v, None),
        span,
    }
}

fn call(name: &str, args: Vec<Expr>, span: (usize, usize)) -> Expr {
    Expr {
        node: Node::Call(name.into(), args, vec![]),
        span,
    }
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    let span = a.span;
    Expr {
        node: Node::Bin(op, Box::new(a), Box::new(b)),
        span,
    }
}

/// The value of a number literal without a unit.
fn plain(e: &Expr) -> Option<f64> {
    match e.node {
        Node::Num(v, None) => Some(v),
        _ => None,
    }
}

fn is_zero(e: &Expr) -> bool {
    matches!(e.node, Node::Num(v, _) if v == 0.0)
}

fn fold(op: BinOp, a: Expr, b: Expr) -> Expr {
    match (plain(&a), plain(&b)) {
        (Some(x), Some(y)) => num(crate::arith(op, x, y), a.span),
        _ => bin(op, a, b),
    }
}

fn add(a: Expr, b: Expr) -> Expr {
    match (is_zero(&a), is_zero(&b)) {
        (true, _) => b,
        (_, true) => a,
        _ => fold(BinOp::Add, a, b),
    }
}

fn sub(a: Expr, b: Expr) -> Expr {
    match (is_zero(&a), is_zero(&b)) {
        (_, true) => a,
        (true, _) => neg(b),
        _ => fold(BinOp::Sub, a, b),
    }
}

fn mul(a: Expr, b: Expr) -> Expr {
    if is_zero(&a) || is_zero(&b) {
        return num(0.0, a.span);
    }
    match (plain(&a), plain(&b)) {
        (Some(1.0), _) => b,
        (_, Some(1.0)) => a,
        _ => fold(BinOp::Mul, a, b),
    }
}

fn div(a: Expr, b: Expr) -> Expr {
    if is_zero(&a) {
        return a;
    }
    match plain(&b) {
        Some(1.0) => a,
        _ => fold(BinOp::Div, a, b),
    }
}

fn pow(a: Expr, b: Expr) -> Expr {
    match plain(&b) {
        Some(1.0) => a,
        _ => fold(BinOp::Pow, a, b),
    }
}

fn neg(a: Expr) -> Expr {
    match plain(&a) {
        Some(x) => num(-x, a.span),
        None => {
            let span = a.span;
            Expr {
                node: Node::Neg(Box::new(a)),
                span,
            }
        }
    }
}
