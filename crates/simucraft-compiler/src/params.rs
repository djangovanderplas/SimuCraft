//! Parameter evaluation. Model parameters are evaluated on first use, so a
//! parameter that no block uses (for example a data file) costs nothing.
//! Values are in coherent SI units (see `simucraft-expr`).

use std::cell::RefCell;
use std::collections::BTreeMap;

use simucraft_contracts::Diagnostic;
use simucraft_contracts::ir::ParamValue;
use simucraft_expr::{Unit, Value, check_units, eval, parse};
use simucraft_format::schema::{self, Model};

use crate::codes;

/// A value and its unit (`None`: unspecified).
pub type Entry = (Value, Option<Unit>);

enum Def {
    Literal(schema::Value, Option<Unit>),
    Expr(String, Option<Unit>),
    File,
    BadUnit(String),
}

/// The parameters one model instance sees.
pub struct Scope {
    defs: BTreeMap<String, Def>,
    /// Values a parent subsystem block gives, evaluated in the parent.
    overrides: BTreeMap<String, Entry>,
    memo: RefCell<BTreeMap<String, Entry>>,
    active: RefCell<Vec<String>>,
}

impl Scope {
    pub fn new(model: &Model, overrides: BTreeMap<String, Entry>) -> Self {
        let defs = model
            .parameters
            .iter()
            .map(|(name, p)| {
                let unit = match p.unit.as_deref().map(Unit::parse) {
                    Some(Err(m)) => return (name.clone(), Def::BadUnit(m)),
                    Some(Ok(u)) => Some(u),
                    None => None,
                };
                let def = match (&p.value, &p.expr) {
                    (Some(v), _) => Def::Literal(v.clone(), unit),
                    (None, Some(e)) => Def::Expr(e.clone(), unit),
                    (None, None) => Def::File,
                };
                (name.clone(), def)
            })
            .collect();
        Self {
            defs,
            overrides,
            memo: RefCell::default(),
            active: RefCell::default(),
        }
    }

    /// The value of a model parameter, `Ok(None)` if there is none.
    fn get(&self, name: &str) -> Result<Option<Entry>, Diagnostic> {
        if let Some(e) = self.overrides.get(name) {
            return Ok(Some(e.clone()));
        }
        if let Some(e) = self.memo.borrow().get(name) {
            return Ok(Some(e.clone()));
        }
        let Some(def) = self.defs.get(name) else {
            return Ok(None);
        };
        if self.active.borrow().iter().any(|a| a == name) {
            let chain = self.active.borrow().join(" -> ");
            return Err(Diagnostic::error(
                codes::PARAM,
                format!("parameters refer to each other: {chain} -> {name}"),
            ));
        }
        self.active.borrow_mut().push(name.into());
        let entry = match def {
            Def::Literal(v, unit) => self.literal(v, *unit),
            Def::Expr(src, declared) => self.expr(src).and_then(|(v, u)| match (u, declared) {
                (Some(u), Some(d)) if !u.same_dimension(d) => Err(Diagnostic::error(
                    codes::UNIT,
                    format!("`{src}` has unit {u}, but the parameter declares {d}"),
                )),
                _ => Ok((v, declared.or(u))),
            }),
            Def::File => Err(Diagnostic::error(
                codes::PARAM,
                "data-file parameters are not supported yet",
            )),
            Def::BadUnit(m) => Err(Diagnostic::error(codes::PARAM, m.clone())),
        };
        self.active.borrow_mut().pop();
        let entry = entry.map_err(|mut d| {
            d.message = format!("parameter `{name}`: {}", d.message);
            d
        })?;
        self.memo.borrow_mut().insert(name.into(), entry.clone());
        Ok(Some(entry))
    }

    /// Parse, unit-check and evaluate an expression against this scope.
    fn expr(&self, src: &str) -> Result<Entry, Diagnostic> {
        let e = parse(src)?;
        let err: RefCell<Option<Diagnostic>> = RefCell::default();
        let lookup = |n: &str| match self.get(n) {
            Ok(found) => found,
            Err(d) => {
                err.borrow_mut().get_or_insert(d);
                Some((Value::Num(f64::NAN), None)) // already reported; avoid a second "unknown name"
            }
        };
        let unit = check_units(&e, &|n| lookup(n).map(|x| x.1));
        let value = unit.and_then(|u| eval(&e, &|n| lookup(n).map(|x| x.0)).map(|v| (v, u)));
        match err.into_inner() {
            Some(d) => Err(d),
            None => value,
        }
    }

    /// A literal from a file, scaled to SI by `unit`. A string is an expression.
    fn literal(&self, v: &schema::Value, unit: Option<Unit>) -> Result<Entry, Diagnostic> {
        let scale = unit.map_or(1.0, |u| u.scale);
        fn value(v: &schema::Value, scale: f64) -> Option<Value> {
            Some(match v {
                schema::Value::Int(i) => Value::Num(*i as f64 * scale),
                schema::Value::Float(x) => Value::Num(x * scale),
                schema::Value::Bool(b) => Value::Bool(*b),
                schema::Value::Array(items) => {
                    Value::List(items.iter().map(|i| value(i, scale)).collect::<Option<_>>()?)
                }
                schema::Value::Text(_) => return None,
            })
        }
        match v {
            schema::Value::Text(src) => self.expr(src),
            _ => value(v, scale)
                .map(|v| (v, unit))
                .ok_or_else(|| Diagnostic::error(codes::PARAM, "an array element is a string")),
        }
    }

    /// A value a subsystem block passes to its child model.
    pub fn literal_or_expr(&self, v: &schema::Value) -> Result<Entry, Diagnostic> {
        self.literal(v, None)
    }

    /// A block parameter. It is text if the block type's default is text;
    /// otherwise a string is an expression.
    pub fn block_param(&self, v: &schema::Value, default: Option<&str>) -> Result<ParamValue, Diagnostic> {
        let text = default.is_some_and(|d| d.parse::<f64>().is_err() && !d.starts_with('['));
        match (v, text) {
            (schema::Value::Text(s), true) => Ok(ParamValue::Text(s.clone())),
            (_, true) => Err(Diagnostic::error(codes::PARAM, "this parameter must be a string")),
            _ => to_param(&self.literal(v, None)?.0),
        }
    }
}

fn to_param(v: &Value) -> Result<ParamValue, Diagnostic> {
    let num = |v: &Value| match v {
        Value::Num(x) => Some(*x),
        _ => None,
    };
    let bad = || {
        Diagnostic::error(
            codes::PARAM,
            "the value must be a number, a boolean, a vector or a matrix",
        )
    };
    match v {
        Value::Num(x) => Ok(ParamValue::Scalar(*x)),
        Value::Bool(b) => Ok(ParamValue::Bool(*b)),
        Value::List(items) => {
            if let Some(xs) = items.iter().map(num).collect::<Option<Vec<f64>>>() {
                return Ok(ParamValue::Vector(xs));
            }
            let rows: Vec<Vec<f64>> = items
                .iter()
                .map(|r| match r {
                    Value::List(cells) => cells.iter().map(num).collect::<Option<Vec<f64>>>(),
                    _ => None,
                })
                .collect::<Option<_>>()
                .ok_or_else(bad)?;
            let cols = rows.first().map_or(0, Vec::len);
            if rows.iter().any(|r| r.len() != cols) {
                return Err(bad());
            }
            Ok(ParamValue::Matrix {
                rows: rows.len(),
                cols,
                data: rows.concat(),
            })
        }
        Value::Signal(_) => Err(bad()),
    }
}
