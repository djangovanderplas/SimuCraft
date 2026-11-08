//! Units: UCUM-style strings (`"V*s/rad"`, `"kg*m2"`, `"1"`) parsed to a
//! scale factor and a dimension vector.

use std::fmt;

/// Base dimensions, in the order of [`Unit::dims`]. `rad` is a base unit, as in UCUM.
const BASE: [&str; 8] = ["m", "kg", "s", "A", "K", "mol", "cd", "rad"];

/// A unit: `value_in_si = value * scale`, with exponents of the [`BASE`] dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    /// Factor to coherent SI (`ms` is 1e-3).
    pub scale: f64,
    /// Exponents of m, kg, s, A, K, mol, cd, rad.
    pub dims: [i8; 8],
}

/// Atoms: symbol, scale, dims.
#[rustfmt::skip]
const ATOMS: &[(&str, f64, [i8; 8])] = &[
    ("m", 1.0, [1, 0, 0, 0, 0, 0, 0, 0]),
    ("g", 1e-3, [0, 1, 0, 0, 0, 0, 0, 0]),
    ("s", 1.0, [0, 0, 1, 0, 0, 0, 0, 0]),
    ("A", 1.0, [0, 0, 0, 1, 0, 0, 0, 0]),
    ("K", 1.0, [0, 0, 0, 0, 1, 0, 0, 0]),
    ("mol", 1.0, [0, 0, 0, 0, 0, 1, 0, 0]),
    ("cd", 1.0, [0, 0, 0, 0, 0, 0, 1, 0]),
    ("rad", 1.0, [0, 0, 0, 0, 0, 0, 0, 1]),
    ("deg", std::f64::consts::PI / 180.0, [0, 0, 0, 0, 0, 0, 0, 1]),
    ("min", 60.0, [0, 0, 1, 0, 0, 0, 0, 0]),
    ("h", 3600.0, [0, 0, 1, 0, 0, 0, 0, 0]),
    ("Hz", 1.0, [0, 0, -1, 0, 0, 0, 0, 0]),
    ("N", 1.0, [1, 1, -2, 0, 0, 0, 0, 0]),
    ("Pa", 1.0, [-1, 1, -2, 0, 0, 0, 0, 0]),
    ("J", 1.0, [2, 1, -2, 0, 0, 0, 0, 0]),
    ("W", 1.0, [2, 1, -3, 0, 0, 0, 0, 0]),
    ("C", 1.0, [0, 0, 1, 1, 0, 0, 0, 0]),
    ("V", 1.0, [2, 1, -3, -1, 0, 0, 0, 0]),
    ("Ohm", 1.0, [2, 1, -3, -2, 0, 0, 0, 0]),
    ("S", 1.0, [-2, -1, 3, 2, 0, 0, 0, 0]),
    ("F", 1.0, [-2, -1, 4, 2, 0, 0, 0, 0]),
    ("Wb", 1.0, [2, 1, -2, -1, 0, 0, 0, 0]),
    ("T", 1.0, [0, 1, -2, -1, 0, 0, 0, 0]),
    ("H", 1.0, [2, 1, -2, -2, 0, 0, 0, 0]),
    ("L", 1e-3, [3, 0, 0, 0, 0, 0, 0, 0]),
];

const PREFIXES: &[(&str, f64)] = &[
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
];

impl Unit {
    /// Dimensionless, scale 1.
    pub const ONE: Unit = Unit {
        scale: 1.0,
        dims: [0; 8],
    };

    /// Parse a unit string: terms joined by `*` (or `.`) and `/`, left to
    /// right; each term is an optional prefix, an atom and an integer exponent
    /// (`m2`, `s-1`).
    pub fn parse(text: &str) -> Result<Unit, String> {
        let mut unit = Unit::ONE;
        let mut divide = false;
        let mut start = 0;
        for (i, c) in text.char_indices().chain([(text.len(), '*')]) {
            if c == '*' || c == '.' || c == '/' {
                let term =
                    term(&text[start..i]).ok_or_else(|| format!("unknown unit `{}` in `{text}`", &text[start..i]))?;
                unit = if divide { unit / term } else { unit * term };
                divide = c == '/';
                start = i + 1;
            }
        }
        Ok(unit)
    }

    /// Same dimension, whatever the scale.
    pub fn same_dimension(&self, other: &Unit) -> bool {
        self.dims == other.dims
    }

    /// True if all exponents are zero.
    pub fn is_dimensionless(&self) -> bool {
        self.dims == [0; 8]
    }

    /// Integer power.
    pub fn powi(self, n: i32) -> Unit {
        Unit {
            scale: self.scale.powi(n),
            dims: self.dims.map(|d| d * n as i8),
        }
    }
}

fn term(text: &str) -> Option<Unit> {
    if text == "1" {
        return Some(Unit::ONE);
    }
    let body = text.trim_end_matches(|c: char| c.is_ascii_digit());
    let symbol = body.strip_suffix(['-', '+']).unwrap_or(body);
    let exp: i32 = if symbol.len() == text.len() {
        1
    } else {
        text[symbol.len()..].parse().ok()?
    };
    let atom = |s: &str| ATOMS.iter().find(|a| a.0 == s).map(|a| Unit { scale: a.1, dims: a.2 });
    let unit = atom(symbol).or_else(|| {
        PREFIXES.iter().find_map(|(p, f)| {
            atom(symbol.strip_prefix(p)?).map(|a| Unit {
                scale: a.scale * f,
                ..a
            })
        })
    })?;
    Some(unit.powi(exp))
}

impl std::ops::Mul for Unit {
    type Output = Unit;
    // Exponents add when units multiply.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, o: Unit) -> Unit {
        Unit {
            scale: self.scale * o.scale,
            dims: std::array::from_fn(|i| self.dims[i] + o.dims[i]),
        }
    }
}

impl std::ops::Div for Unit {
    type Output = Unit;
    fn div(self, o: Unit) -> Unit {
        self * o.powi(-1)
    }
}

/// Coherent SI form, for messages: `m2*kg/s3/A`.
impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let part = |i: usize, d: i8| {
            if d.abs() == 1 {
                BASE[i].to_string()
            } else {
                format!("{}{}", BASE[i], d.abs())
            }
        };
        let up: Vec<String> = (0..8)
            .filter(|&i| self.dims[i] > 0)
            .map(|i| part(i, self.dims[i]))
            .collect();
        let down: Vec<String> = (0..8)
            .filter(|&i| self.dims[i] < 0)
            .map(|i| part(i, self.dims[i]))
            .collect();
        let mut s = if up.is_empty() { "1".to_string() } else { up.join("*") };
        down.iter().for_each(|d| s += &format!("/{d}"));
        if self.scale != 1.0 {
            s = format!("{}*{s}", self.scale);
        }
        f.write_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_units() {
        let u = |s| Unit::parse(s).unwrap();
        assert_eq!(u("V*s/rad") * u("rad/s"), u("V"));
        assert_eq!(u("Ohm") * u("A"), u("V"));
        assert_eq!(u("kg*m2").dims, [2, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(u("ms").scale, 1e-3);
        assert_eq!(u("mm").scale, 1e-3);
        assert_eq!(u("kg").scale, 1.0);
        assert_eq!(u("s-1"), u("Hz"));
        assert_eq!(u("1"), Unit::ONE);
        assert_eq!(u("N*m*s/rad").to_string(), "m2*kg/s/rad");
        assert!(Unit::parse("furlong").is_err());
        assert!(Unit::parse("").is_err());
    }
}
