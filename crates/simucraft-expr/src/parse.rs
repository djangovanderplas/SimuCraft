//! Lexer and recursive-descent parser. Precedence follows Python.

use simucraft_contracts::Diagnostic;

use crate::{BinOp, Expr, Node, Unit, codes, error};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64, Option<(String, Unit)>),
    Name(String),
    Sym(&'static str),
    End,
}

const SYMBOLS: [&str; 18] = [
    "**", "<=", ">=", "==", "!=", "+", "-", "*", "/", "%", "<", ">", "(", ")", "[", "]", ",", "=",
];

const KEYWORDS: [&str; 7] = ["and", "or", "not", "if", "else", "True", "False"];

fn lex(src: &str) -> Result<Vec<(Tok, usize, usize)>, Diagnostic> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] | 0x20) == b'e' {
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_digit() {
                    i = j;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let value: f64 = src[start..i]
                .parse()
                .map_err(|_| error(codes::SYNTAX, format!("bad number `{}`", &src[start..i]), start))?;
            let mut unit = None;
            if bytes.get(i) == Some(&b'[') {
                let close = src[i..]
                    .find(']')
                    .ok_or_else(|| error(codes::SYNTAX, "unit bracket `[` has no `]`", i))?;
                let text = &src[i + 1..i + close];
                let parsed = Unit::parse(text).map_err(|m| error(codes::BAD_UNIT, m, i + 1))?;
                unit = Some((text.to_string(), parsed));
                i += close + 1;
            }
            out.push((Tok::Num(value, unit), start, i));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push((Tok::Name(src[start..i].to_string()), start, i));
        } else if let Some(s) = SYMBOLS.iter().find(|s| src[i..].starts_with(**s)) {
            i += s.len();
            out.push((Tok::Sym(s), start, i));
        } else {
            let ch = src[i..].chars().next().unwrap_or('?');
            return Err(error(codes::SYNTAX, format!("unexpected character `{ch}`"), i));
        }
    }
    out.push((Tok::End, src.len(), src.len()));
    Ok(out)
}

/// Parse one expression.
pub fn parse(src: &str) -> Result<Expr, Diagnostic> {
    let mut p = Parser {
        toks: lex(src)?,
        pos: 0,
    };
    let e = p.expr()?;
    match p.peek() {
        Tok::End => Ok(e),
        _ => Err(p.unexpected()),
    }
}

/// Positional and keyword arguments of a call.
type Args = (Vec<Expr>, Vec<(String, Expr)>);

struct Parser {
    toks: Vec<(Tok, usize, usize)>,
    pos: usize,
}

fn mk(node: Node, start: usize, end: usize) -> Expr {
    Expr {
        node,
        span: (start, end),
    }
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    let span = (a.span.0, b.span.1);
    Expr {
        node: Node::Bin(op, Box::new(a), Box::new(b)),
        span,
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }
    fn start(&self) -> usize {
        self.toks[self.pos].1
    }
    fn prev_end(&self) -> usize {
        self.toks[self.pos - 1].2
    }
    fn next(&mut self) -> Tok {
        self.pos += 1;
        self.toks[self.pos - 1].0.clone()
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(t) if *t == s)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Name(n) if n == k)
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let hit = self.is_sym(s);
        self.pos += hit as usize;
        hit
    }
    fn expect(&mut self, s: &str) -> Result<(), Diagnostic> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }
    fn unexpected(&self) -> Diagnostic {
        let what = match self.peek() {
            Tok::End => "end of expression".to_string(),
            Tok::Num(v, _) => format!("number `{v}`"),
            Tok::Name(n) => format!("`{n}`"),
            Tok::Sym(s) => format!("`{s}`"),
        };
        error(codes::SYNTAX, format!("unexpected {what}"), self.start())
    }

    fn expr(&mut self) -> Result<Expr, Diagnostic> {
        let then = self.or()?;
        if !self.is_kw("if") {
            return Ok(then);
        }
        self.pos += 1;
        let cond = self.or()?;
        if !self.is_kw("else") {
            return Err(self.unexpected());
        }
        self.pos += 1;
        let other = self.expr()?;
        let span = (then.span.0, other.span.1);
        Ok(mk(
            Node::If(Box::new(then), Box::new(cond), Box::new(other)),
            span.0,
            span.1,
        ))
    }

    fn or(&mut self) -> Result<Expr, Diagnostic> {
        let mut a = self.and()?;
        while self.is_kw("or") {
            self.pos += 1;
            a = bin(BinOp::Or, a, self.and()?);
        }
        Ok(a)
    }

    fn and(&mut self) -> Result<Expr, Diagnostic> {
        let mut a = self.not()?;
        while self.is_kw("and") {
            self.pos += 1;
            a = bin(BinOp::And, a, self.not()?);
        }
        Ok(a)
    }

    fn not(&mut self) -> Result<Expr, Diagnostic> {
        if !self.is_kw("not") {
            return self.comparison();
        }
        let start = self.start();
        self.pos += 1;
        let e = self.not()?;
        let end = e.span.1;
        Ok(mk(Node::Not(Box::new(e)), start, end))
    }

    fn comparison(&mut self) -> Result<Expr, Diagnostic> {
        let a = self.sum()?;
        let op = |p: &Self| {
            [
                ("<", BinOp::Lt),
                ("<=", BinOp::Le),
                (">", BinOp::Gt),
                (">=", BinOp::Ge),
                ("==", BinOp::Eq),
                ("!=", BinOp::Ne),
            ]
            .into_iter()
            .find(|(s, _)| p.is_sym(s))
            .map(|(_, op)| op)
        };
        let Some(o) = op(self) else { return Ok(a) };
        self.pos += 1;
        let e = bin(o, a, self.sum()?);
        if op(self).is_some() {
            return Err(error(
                codes::SYNTAX,
                "chained comparisons are not supported; use `and`",
                self.start(),
            ));
        }
        Ok(e)
    }

    fn sum(&mut self) -> Result<Expr, Diagnostic> {
        let mut a = self.term()?;
        loop {
            let op = if self.eat_sym("+") {
                BinOp::Add
            } else if self.eat_sym("-") {
                BinOp::Sub
            } else {
                return Ok(a);
            };
            a = bin(op, a, self.term()?);
        }
    }

    fn term(&mut self) -> Result<Expr, Diagnostic> {
        let mut a = self.unary()?;
        loop {
            let op = if self.eat_sym("*") {
                BinOp::Mul
            } else if self.eat_sym("/") {
                BinOp::Div
            } else if self.eat_sym("%") {
                BinOp::Rem
            } else {
                return Ok(a);
            };
            a = bin(op, a, self.unary()?);
        }
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.start();
        if self.eat_sym("-") {
            let e = self.unary()?;
            let end = e.span.1;
            return Ok(mk(Node::Neg(Box::new(e)), start, end));
        }
        if self.eat_sym("+") {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, Diagnostic> {
        let base = self.postfix()?;
        if self.eat_sym("**") {
            // Right-associative, and binds tighter than a unary minus on its left.
            return Ok(bin(BinOp::Pow, base, self.unary()?));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut e = self.atom()?;
        loop {
            if self.is_sym("(") {
                let Node::Name(name) = &e.node else {
                    return Err(error(codes::SYNTAX, "only a function name can be called", self.start()));
                };
                let name = name.clone();
                self.pos += 1;
                let (args, kwargs) = self.args()?;
                e = mk(Node::Call(name, args, kwargs), e.span.0, self.prev_end());
            } else if self.eat_sym("[") {
                let index = self.expr()?;
                self.expect("]")?;
                e = mk(
                    Node::Index(Box::new(e.clone()), Box::new(index)),
                    e.span.0,
                    self.prev_end(),
                );
            } else {
                return Ok(e);
            }
        }
    }

    /// Call arguments after `(`, up to and including `)`.
    fn args(&mut self) -> Result<Args, Diagnostic> {
        let (mut args, mut kwargs) = (Vec::new(), Vec::new());
        while !self.eat_sym(")") {
            if !args.is_empty() || !kwargs.is_empty() {
                self.expect(",")?;
            }
            let keyword = matches!(self.peek(), Tok::Name(_)) && matches!(self.toks[self.pos + 1].0, Tok::Sym("="));
            if keyword {
                let Tok::Name(k) = self.next() else { unreachable!() };
                self.pos += 1;
                kwargs.push((k, self.expr()?));
            } else if !kwargs.is_empty() {
                return Err(error(
                    codes::SYNTAX,
                    "a positional argument cannot follow a keyword argument",
                    self.start(),
                ));
            } else {
                args.push(self.expr()?);
            }
        }
        Ok((args, kwargs))
    }

    fn atom(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.start();
        let node = match self.next() {
            Tok::Num(v, unit) => Node::Num(v, unit),
            Tok::Name(n) if n == "True" => Node::Bool(true),
            Tok::Name(n) if n == "False" => Node::Bool(false),
            Tok::Name(n) if KEYWORDS.contains(&n.as_str()) => {
                self.pos -= 1;
                return Err(self.unexpected());
            }
            Tok::Name(n) => Node::Name(n),
            Tok::Sym("(") => {
                // The span stays on the inner expression, so errors point inside the parentheses.
                let e = self.expr()?;
                self.expect(")")?;
                return Ok(e);
            }
            Tok::Sym("[") => {
                let mut items = Vec::new();
                while !self.eat_sym("]") {
                    if !items.is_empty() {
                        self.expect(",")?;
                    }
                    items.push(self.expr()?);
                }
                Node::List(items)
            }
            _ => {
                self.pos -= 1;
                return Err(self.unexpected());
            }
        };
        Ok(mk(node, start, self.prev_end()))
    }
}
