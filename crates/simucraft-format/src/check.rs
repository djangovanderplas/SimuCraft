//! File checks: TOML syntax, the `format` line, no inline tables, the schema,
//! and the per-file rules that need no other file.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::de::DeserializeOwned;
use simucraft_contracts::{Diagnostic, Span};
use toml_edit::{Document, Item, Value};

use crate::schema::{Design, Experiment, Layout, Lock, Model, Project};
use crate::{FORMAT_VERSION, Kind, id};

/// Diagnostic codes reported by this crate.
pub mod codes {
    /// Not valid TOML.
    pub const SYNTAX: &str = "F001";
    /// `format` line missing, not first, or for another file kind.
    pub const FORMAT_LINE: &str = "F002";
    /// Format version this build cannot read.
    pub const VERSION: &str = "F003";
    /// Inline table; write a `[sub.table]` instead.
    pub const INLINE_TABLE: &str = "F004";
    /// Unknown key, missing key or wrong value type.
    pub const SCHEMA: &str = "F005";
    /// Malformed stable ID.
    pub const BAD_ID: &str = "F006";
    /// Two items with the same name on one model level.
    pub const DUPLICATE_NAME: &str = "F007";
    /// Reference to a block that does not exist in this file.
    pub const DANGLING: &str = "F008";
    /// Name that cannot be used in a path.
    pub const BAD_NAME: &str = "F009";
    /// Fields that do not fit together.
    pub const FIELDS: &str = "F010";
}

/// Check one file's text. An empty result means the file is valid.
pub fn check(kind: Kind, text: &str) -> Vec<Diagnostic> {
    let doc = match Document::parse(text) {
        Ok(doc) => doc,
        Err(e) => {
            let d = Diagnostic::error(codes::SYNTAX, e.message().to_string());
            return vec![d.span(e.span().map(|r| span_of(text, r)))];
        }
    };
    let mut out = Vec::new();
    if !format_line(&doc, kind, text, &mut out) {
        return out;
    }
    inline_tables(doc.as_table(), "", text, &mut out);
    match kind {
        Kind::Project => typed::<Project>(text, &mut out).map(|p| project(&p, &mut out)),
        Kind::Lock => typed::<Lock>(text, &mut out).map(|_| ()),
        Kind::Model => typed::<Model>(text, &mut out).map(|m| model(&m, &mut out)),
        Kind::Layout => typed::<Layout>(text, &mut out).map(|l| layout(&l, &mut out)),
        Kind::Experiment => typed::<Experiment>(text, &mut out).map(|e| experiment(&e, &mut out)),
        Kind::Design => typed::<Design>(text, &mut out).map(|_| ()),
    };
    out
}

fn span_of(text: &str, range: Range<usize>) -> Span {
    let before = &text[..range.start.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    Span {
        line: line as u32,
        col: col as u32,
    }
}

/// The first key must be `format = "simucraft.<kind>/<version>"`.
fn format_line(doc: &Document<&str>, kind: Kind, text: &str, out: &mut Vec<Diagnostic>) -> bool {
    let want = format!("simucraft.{}/{FORMAT_VERSION}", kind.name());
    let first = doc.as_table().iter().next();
    let Some(("format", item)) = first else {
        out.push(
            Diagnostic::error(codes::FORMAT_LINE, "the first key must be `format`")
                .span(Some(Span { line: 1, col: 1 }))
                .fix(format!("add `format = \"{want}\"` as the first line")),
        );
        return false;
    };
    let span = item.span().map(|r| span_of(text, r));
    let Some(value) = item.as_str() else {
        out.push(
            Diagnostic::error(codes::FORMAT_LINE, "`format` must be a string")
                .at("format")
                .span(span),
        );
        return false;
    };
    let (Some(name), Some(version)) = (
        value.strip_prefix("simucraft.").and_then(|v| v.split('/').next()),
        value.split_once('/').map(|(_, v)| v),
    ) else {
        out.push(
            Diagnostic::error(
                codes::FORMAT_LINE,
                format!("`{value}` is not `simucraft.<kind>/<version>`"),
            )
            .at("format")
            .span(span),
        );
        return false;
    };
    if name != kind.name() {
        out.push(
            Diagnostic::error(
                codes::FORMAT_LINE,
                format!("this is a {name} file, expected a {} file", kind.name()),
            )
            .at("format")
            .span(span),
        );
        return false;
    }
    if version != FORMAT_VERSION {
        out.push(
            Diagnostic::error(
                codes::VERSION,
                format!("format version {version} is not supported; this build reads {FORMAT_VERSION}"),
            )
            .at("format")
            .span(span),
        );
        return false;
    }
    true
}

fn inline_tables(table: &toml_edit::Table, path: &str, text: &str, out: &mut Vec<Diagnostic>) {
    for (key, item) in table.iter() {
        let here = if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        };
        match item {
            Item::Table(t) => inline_tables(t, &here, text, out),
            Item::Value(v) => inline_values(v, &here, text, out),
            Item::ArrayOfTables(a) => a.iter().for_each(|t| inline_tables(t, &here, text, out)),
            Item::None => {}
        }
    }
}

fn inline_values(value: &Value, path: &str, text: &str, out: &mut Vec<Diagnostic>) {
    match value {
        Value::InlineTable(_) => out.push(
            Diagnostic::error(codes::INLINE_TABLE, "inline tables are not allowed")
                .at(path)
                .span(value.span().map(|r| span_of(text, r)))
                .fix(format!("write the values under a `[{path}]` table, one key per line")),
        ),
        Value::Array(a) => a.iter().for_each(|v| inline_values(v, path, text, out)),
        _ => {}
    }
}

fn typed<T: DeserializeOwned>(text: &str, out: &mut Vec<Diagnostic>) -> Option<T> {
    toml_edit::de::from_str(text)
        .map_err(|e| {
            let message = e.message().trim().to_string();
            out.push(Diagnostic::error(codes::SCHEMA, message).span(e.span().map(|r| span_of(text, r))));
        })
        .ok()
}

fn bad_id(path: &str, id: &str, prefix: char) -> Diagnostic {
    Diagnostic::error(
        codes::BAD_ID,
        format!("`{id}` is not a valid ID: expected `{prefix}_` and 12 lowercase base32 characters"),
    )
    .at(path)
}

/// A port reference `<block id>.<port>` whose block must exist.
fn port_ref(path: &str, reference: &str, blocks: &BTreeSet<&str>, out: &mut Vec<Diagnostic>) {
    match reference.split_once('.') {
        Some((block, port)) if !port.is_empty() && blocks.contains(block) => {}
        Some((block, port)) if !port.is_empty() => out.push(
            Diagnostic::error(
                codes::DANGLING,
                format!("`{reference}` names block `{block}`, which is not in this model"),
            )
            .at(path)
            .id(block),
        ),
        _ => out.push(Diagnostic::error(codes::DANGLING, format!("`{reference}` is not `<block id>.<port>`")).at(path)),
    }
}

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn project(p: &Project, out: &mut Vec<Diagnostic>) {
    if !p.top.ends_with(".scmodel") {
        out.push(Diagnostic::error(codes::FIELDS, "`top` must point to a .scmodel file").at("top"));
    }
}

fn model(m: &Model, out: &mut Vec<Diagnostic>) {
    if !id::is_valid(&m.id, 'm') {
        out.push(bad_id("id", &m.id, 'm'));
    }
    for (name, p) in &m.parameters {
        let path = format!("parameters.{name}");
        if !is_name(name) {
            out.push(Diagnostic::error(codes::BAD_NAME, format!("`{name}` is not a valid parameter name")).at(&path));
        }
        let sources = [p.value.is_some(), p.expr.is_some(), p.file.is_some()];
        if sources.iter().filter(|s| **s).count() != 1 {
            out.push(
                Diagnostic::error(
                    codes::FIELDS,
                    "a parameter needs exactly one of `value`, `expr` or `file`",
                )
                .at(&path),
            );
        }
    }

    let blocks: BTreeSet<&str> = m.blocks.keys().map(String::as_str).collect();
    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    for (bid, b) in &m.blocks {
        let path = format!("blocks.{bid}");
        if !id::is_valid(bid, 'b') {
            out.push(bad_id(&path, bid, 'b'));
        }
        if !is_name(&b.name) {
            out.push(
                Diagnostic::error(codes::BAD_NAME, format!("`{}` is not a valid block name", b.name))
                    .at(&path)
                    .id(bid),
            );
        }
        if let Some(other) = names.insert(&b.name, bid) {
            out.push(
                Diagnostic::error(
                    codes::DUPLICATE_NAME,
                    format!("block name `{}` is also used by `{other}`", b.name),
                )
                .at(&path)
                .id(bid),
            );
        }
        if (b.kind == "subsystem") != b.reference.is_some() {
            out.push(
                Diagnostic::error(codes::FIELDS, "`ref` is required for subsystems and only allowed there")
                    .at(&path)
                    .id(bid),
            );
        }
        for (port, source) in &b.inputs {
            port_ref(&format!("{path}.inputs.{port}"), source, &blocks, out);
        }
    }
    for (nid, net) in &m.nets {
        let path = format!("nets.{nid}");
        if !id::is_valid(nid, 'n') {
            out.push(bad_id(&path, nid, 'n'));
        }
        if net.ports.len() < 2 {
            out.push(
                Diagnostic::error(codes::FIELDS, "a net joins at least two ports")
                    .at(&path)
                    .id(nid),
            );
        }
        for port in &net.ports {
            port_ref(&path, port, &blocks, out);
        }
    }
    for port in m.signals.keys() {
        port_ref(&format!("signals.\"{port}\""), port, &blocks, out);
    }
}

fn layout(l: &Layout, out: &mut Vec<Diagnostic>) {
    let keyed = [
        (l.blocks.keys().collect::<Vec<_>>(), "blocks", 'b'),
        (l.nets.keys().collect(), "nets", 'n'),
        (l.annotations.keys().collect(), "annotations", 'a'),
    ];
    for (keys, table, prefix) in keyed {
        for key in keys {
            if !id::is_valid(key, prefix) {
                out.push(bad_id(&format!("{table}.{key}"), key, prefix));
            }
        }
    }
}

fn experiment(e: &Experiment, out: &mut Vec<Diagnostic>) {
    if e.runs == Some(0) {
        out.push(Diagnostic::error(codes::FIELDS, "`runs` must be at least 1").at("runs"));
    }
    for (name, v) in &e.vary {
        let path = format!("vary.\"{name}\"");
        if v.from_model == v.dist.is_some() && v.values.is_none() {
            out.push(
                Diagnostic::error(
                    codes::FIELDS,
                    "a vary entry needs `from_model = true`, a `dist`, or `values`",
                )
                .at(&path),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes_of(kind: Kind, text: &str) -> Vec<String> {
        check(kind, text).into_iter().map(|d| d.code).collect()
    }

    const MODEL_HEAD: &str = "format = \"simucraft.model/0.1\"\nid = \"m_4c1e9ah2kq7d\"\nname = \"t\"\n";

    #[test]
    fn syntax_error_has_a_span() {
        let d = check(Kind::Model, "format = \"simucraft.model/0.1\"\nname = \n");
        assert_eq!(d[0].code, codes::SYNTAX);
        assert_eq!(d[0].span.unwrap().line, 2);
    }

    #[test]
    fn format_line_is_checked() {
        assert_eq!(codes_of(Kind::Model, "name = \"x\"\n"), [codes::FORMAT_LINE]);
        assert_eq!(codes_of(Kind::Layout, MODEL_HEAD), [codes::FORMAT_LINE]);
        assert_eq!(
            codes_of(Kind::Model, "format = \"simucraft.model/9.0\"\n"),
            [codes::VERSION]
        );
    }

    #[test]
    fn inline_tables_are_rejected() {
        let text =
            format!("{MODEL_HEAD}[blocks.b_4a10f3k8m2qx]\nname = \"g\"\ntype = \"core.Gain\"\nparams = {{ k = 2 }}\n");
        let d = check(Kind::Model, &text);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, codes::INLINE_TABLE);
        assert_eq!(d[0].path.as_deref(), Some("blocks.b_4a10f3k8m2qx.params"));
        assert_eq!(d[0].span.unwrap().line, 7);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let d = check(Kind::Model, &format!("{MODEL_HEAD}colour = \"red\"\n"));
        assert_eq!(d[0].code, codes::SCHEMA);
        assert!(d[0].message.contains("colour"), "{}", d[0].message);
        assert_eq!(d[0].span.unwrap().line, 4);
    }

    #[test]
    fn model_rules() {
        let text = format!(
            "{MODEL_HEAD}
[blocks.b_short]
name = \"a\"
type = \"core.Gain\"

[blocks.b_4a10f3k8m2qx]
name = \"a\"
type = \"core.Gain\"

[blocks.b_4a10f3k8m2qx.inputs]
u = \"b_zzzzzzzzzzzz.y\"
"
        );
        let mut got = codes_of(Kind::Model, &text);
        got.sort();
        assert_eq!(got, [codes::BAD_ID, codes::DUPLICATE_NAME, codes::DANGLING]);
    }
}
