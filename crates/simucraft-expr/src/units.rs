//! Unit checking and inference (spec Section 9, "Units policy").
//!
//! `None` is an unspecified unit: it meets anything without a diagnostic, and
//! it makes the result unspecified. A number without `[unit]` is unspecified
//! (owner decision 2026-11-08), so `x + 1` is valid and `2 * x` is unspecified.

use simucraft_contracts::Diagnostic;

use crate::{BinOp, Expr, Node, Unit, check_call, codes, constant};

type Units = Option<Unit>;

/// Check the units of `e` and return the unit of the result. `unit_of`
/// gives the unit of each name: `None` if the name is unknown,
/// `Some(None)` if its unit is unspecified.
pub fn check_units(e: &Expr, unit_of: &dyn Fn(&str) -> Option<Units>) -> Result<Units, Diagnostic> {
    let go = |e: &Expr| check_units(e, unit_of);
    let rad = Unit::parse("rad").expect("rad is an atom");
    Ok(match &e.node {
        Node::Num(_, unit) => unit.as_ref().map(|u| u.1),
        Node::Bool(_) | Node::Not(_) => Some(Unit::ONE),
        Node::Name(n) => match unit_of(n) {
            Some(u) => u,
            None if constant(n).is_some() => Some(Unit::ONE),
            None => return Err(e.error(codes::UNKNOWN_NAME, format!("unknown name `{n}`"))),
        },
        Node::Neg(a) => go(a)?,
        Node::Bin(op, a, b) => {
            let (ua, ub) = (go(a)?, go(b)?);
            match op {
                BinOp::Mul => ua.zip(ub).map(|(a, b)| a * b),
                BinOp::Div => ua.zip(ub).map(|(a, b)| a / b),
                BinOp::Pow => {
                    dimensionless(b, ub)?;
                    match ua {
                        Some(u) if !u.is_dimensionless() => Some(power(e, b, u)?),
                        other => other,
                    }
                }
                BinOp::And | BinOp::Or => Some(Unit::ONE),
                _ if op.is_comparison() => same(e, ua, ub).map(|_| Some(Unit::ONE))?,
                _ => same(e, ua, ub)?,
            }
        }
        Node::If(then, cond, other) => {
            go(cond)?;
            same(e, go(then)?, go(other)?)?
        }
        Node::Index(a, i) => {
            go(i)?;
            go(a)?
        }
        Node::List(items) => {
            let mut unit = Some(Unit::ONE);
            for (i, item) in items.iter().enumerate() {
                let u = go(item)?;
                unit = if i == 0 { u } else { same(e, unit, u)? };
            }
            unit
        }
        Node::Call(name, args, kwargs) => {
            check_call(e, name, args, kwargs)?;
            for (_, v) in kwargs {
                dimensionless(v, go(v)?)?;
            }
            let units: Vec<Units> = args.iter().map(go).collect::<Result<_, _>>()?;
            let all_same = || units.iter().skip(1).try_fold(units[0], |acc, u| same(e, acc, *u));
            match name.as_str() {
                "abs" | "min" | "max" | "sum" | "mean" | "rms" | "final" | "clip" => all_same()?,
                "sign" => Some(Unit::ONE),
                "sqrt" => match units[0] {
                    Some(u) if u.dims.iter().any(|d| d % 2 != 0) => {
                        return Err(e.error(
                            codes::UNIT,
                            format!("`{e}` takes the root of {u}, which has odd exponents"),
                        ));
                    }
                    u => u.map(|u| Unit {
                        scale: u.scale.sqrt(),
                        dims: u.dims.map(|d| d / 2),
                    }),
                },
                "exp" | "log" => {
                    dimensionless(&args[0], units[0])?;
                    Some(Unit::ONE)
                }
                "sin" | "cos" | "tan" => match units[0] {
                    Some(u) if !u.is_dimensionless() && !u.same_dimension(&rad) => {
                        return Err(args[0].error(codes::UNIT, format!("`{}` must be an angle, not {u}", args[0])));
                    }
                    _ => Some(Unit::ONE),
                },
                "asin" | "acos" | "atan" => {
                    dimensionless(&args[0], units[0])?;
                    Some(rad)
                }
                "atan2" => all_same().map(|_| Some(rad))?,
                "interp" => same(e, units[0], units[1]).map(|_| units[2])?,
                "settling_time" => Some(Unit::parse("s").expect("s is an atom")),
                "der" => units[0].map(|u| u / Unit::parse("s").expect("s is an atom")),
                _ => unreachable!("check_call knows every function"),
            }
        }
    })
}

/// Units that must have the same dimension. The result is unspecified if
/// either one is.
fn same(e: &Expr, a: Units, b: Units) -> Result<Units, Diagnostic> {
    match (a, b) {
        (Some(x), Some(y)) if !x.same_dimension(&y) => Err(e.error(codes::UNIT, format!("`{e}` mixes {x} and {y}"))),
        (Some(x), Some(_)) => Ok(Some(x)),
        _ => Ok(None),
    }
}

fn dimensionless(e: &Expr, u: Units) -> Result<(), Diagnostic> {
    match u {
        Some(u) if !u.is_dimensionless() => Err(e.error(codes::UNIT, format!("`{e}` must be dimensionless, not {u}"))),
        _ => Ok(()),
    }
}

/// `base ** exponent` where the base has a unit: the exponent must be a
/// number literal that keeps the exponents whole.
fn power(e: &Expr, exponent: &Expr, base: Unit) -> Result<Unit, Diagnostic> {
    let n = match &exponent.node {
        Node::Num(v, None) => *v,
        Node::Neg(inner) => match inner.node {
            Node::Num(v, None) => -v,
            _ => f64::NAN,
        },
        _ => f64::NAN,
    };
    if n.fract() != 0.0 || n.is_nan() {
        return Err(e.error(
            codes::UNIT,
            format!("`{e}` raises {base} to `{exponent}`; the exponent must be a whole number literal"),
        ));
    }
    Ok(base.powi(n as i32))
}
