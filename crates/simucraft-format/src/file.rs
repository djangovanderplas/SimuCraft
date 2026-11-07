//! Format-preserving edits. Only what changes is rewritten: comments, blank
//! lines and the order of existing content survive. New tables go in ID order.

use std::fmt;

use simucraft_contracts::Diagnostic;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::{Kind, check};

#[derive(Clone, Debug)]
pub struct File {
    kind: Kind,
    doc: DocumentMut,
}

impl File {
    /// Parse and check `text`. Fails if [`check`] reports any error.
    pub fn parse(kind: Kind, text: &str) -> Result<Self, Vec<Diagnostic>> {
        let diagnostics = check(kind, text);
        if diagnostics.iter().any(Diagnostic::is_error) {
            return Err(diagnostics);
        }
        let doc = text.parse().expect("check() accepted this text");
        Ok(Self { kind, doc })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Set `key = value` in the table at `table`, creating tables as needed.
    /// An existing value keeps its comment and spacing.
    pub fn set(&mut self, table: &[&str], key: &str, value: impl Into<Value>) {
        let mut value = value.into();
        let t = self.table_mut(table);
        match t.get_mut(key).and_then(Item::as_value_mut) {
            Some(old) => {
                *value.decor_mut() = old.decor().clone();
                *old = value;
            }
            None => {
                t.insert(key, Item::Value(value));
            }
        }
    }

    /// Remove the key or table at `path`. Returns whether it existed.
    pub fn remove(&mut self, path: &[&str]) -> bool {
        let Some((key, parent)) = path.split_last() else {
            return false;
        };
        let mut t = self.doc.as_table_mut();
        for k in parent {
            match t.get_mut(k).and_then(Item::as_table_mut) {
                Some(next) => t = next,
                None => return false,
            }
        }
        t.remove(key).is_some()
    }

    /// The table at `path`, created (in ID order) if missing.
    pub fn table_mut(&mut self, path: &[&str]) -> &mut Table {
        for depth in 1..=path.len() {
            let (parent, key) = (&path[..depth - 1], path[depth - 1]);
            let t = table_at(self.doc.as_table_mut(), parent);
            if t.contains_key(key) {
                continue;
            }
            let mut new = Table::new();
            // A table on the way to the target only groups others: no header.
            new.set_implicit(depth < path.len());
            new.decor_mut().set_prefix("\n");
            t.insert(key, Item::Table(new));
            self.place(&path[..depth]);
        }
        table_at(self.doc.as_table_mut(), path)
    }

    /// Give the new table at `path` a position. Entries of a keyed section
    /// (`[blocks.<id>]`, `[parameters.<name>]`) go before the first sibling
    /// with a larger key, which spreads additions from different branches
    /// through the file. Anything else goes after its parent's last table.
    fn place(&mut self, path: &[&str]) {
        let mut order: Vec<Vec<String>> = Vec::new();
        collect(self.doc.as_table(), &mut Vec::new(), &mut order);
        let new: Vec<String> = path.iter().map(|s| s.to_string()).collect();
        order.retain(|p| *p != new);
        let (parent, key) = path.split_at(path.len() - 1);
        let under =
            |p: &Vec<String>, prefix: &[&str]| p.len() >= prefix.len() && p.iter().zip(prefix).all(|(a, b)| a == b);
        let keyed = path.len() == 2;
        let at = order
            .iter()
            .position(|p| keyed && under(p, parent) && p.len() > parent.len() && p[parent.len()].as_str() > key[0])
            .or_else(|| order.iter().rposition(|p| under(p, parent)).map(|i| i + 1))
            .unwrap_or(order.len());
        order.insert(at, new);
        for (i, p) in order.iter().enumerate() {
            let p: Vec<&str> = p.iter().map(String::as_str).collect();
            table_at(self.doc.as_table_mut(), &p).set_position(Some(i as isize));
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}

fn table_at<'a>(mut t: &'a mut Table, path: &[&str]) -> &'a mut Table {
    for k in path {
        t = t[*k].as_table_mut().expect("path names a table");
    }
    t
}

/// Paths of all printed tables, in print order.
fn collect(t: &Table, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let mut found: Vec<(isize, Vec<String>)> = Vec::new();
    walk(t, path, &mut found);
    found.sort_by_key(|(pos, _)| *pos);
    out.extend(found.into_iter().map(|(_, p)| p));
}

fn walk(t: &Table, path: &mut Vec<String>, out: &mut Vec<(isize, Vec<String>)>) {
    for (k, item) in t.iter() {
        if let Item::Table(sub) = item {
            path.push(k.to_string());
            if !sub.is_implicit() || sub.is_empty() {
                out.push((sub.position().unwrap_or(isize::MAX), path.clone()));
            }
            walk(sub, path, out);
            path.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = include_str!("../../../examples/motor_demo/models/speed_control.scmodel");

    fn changed_lines(a: &str, b: &str) -> Vec<String> {
        let old: std::collections::HashSet<&str> = a.lines().collect();
        b.lines().filter(|l| !old.contains(l)).map(String::from).collect()
    }

    #[test]
    fn edit_keeps_comments_and_touches_one_line() {
        let mut f = File::parse(Kind::Model, MODEL).unwrap();
        f.set(&["blocks", "b_5c2d9tz6hyaw"], "ts", 0.002);
        let out = f.to_string();
        assert_eq!(
            changed_lines(MODEL, &out),
            ["ts = 0.002                      # discrete at 1 kHz; omit to inherit"]
        );
        assert_eq!(MODEL.lines().count(), out.lines().count());
        assert!(out.contains("# Signal-flow blocks: each input names its one source"));
    }

    #[test]
    fn new_block_goes_in_id_order() {
        let mut f = File::parse(Kind::Model, MODEL).unwrap();
        let id = "b_4b00000000aa"; // between b_4a10f3k8m2qx and b_5c2d9tz6hyaw
        f.set(&["blocks", id], "name", "gain");
        f.set(&["blocks", id], "type", "core.Gain");
        f.set(&["blocks", id, "params"], "k", 2.0);
        f.set(&["blocks", id, "inputs"], "u", "b_4a10f3k8m2qx.y");
        let out = f.to_string();
        assert_eq!(crate::check(Kind::Model, &out), []);

        let pos = |s: &str| out.find(s).unwrap_or_else(|| panic!("missing {s}\n{out}"));
        assert!(pos("[blocks.b_4a10f3k8m2qx.inputs]") < pos(&format!("[blocks.{id}]")));
        assert!(pos(&format!("[blocks.{id}]")) < pos(&format!("[blocks.{id}.params]")));
        assert!(pos(&format!("[blocks.{id}.params]")) < pos(&format!("[blocks.{id}.inputs]")));
        assert!(pos(&format!("[blocks.{id}.inputs]")) < pos("[blocks.b_5c2d9tz6hyaw]"));
        assert!(out.contains(&format!(
            "\n\n[blocks.{id}]\nname = \"gain\"\ntype = \"core.Gain\"\n\n[blocks.{id}.params]\nk = 2.0\n"
        )));
    }

    #[test]
    fn new_section_goes_at_the_end() {
        let mut f = File::parse(Kind::Model, MODEL).unwrap();
        f.set(&["mask"], "icon", "motor.svg");
        assert!(f.to_string().ends_with("\n\n[mask]\nicon = \"motor.svg\"\n"), "{f}");
    }

    #[test]
    fn remove_block() {
        let mut f = File::parse(Kind::Model, MODEL).unwrap();
        assert!(f.remove(&["blocks", "b_cf0f0q3zc6vn"]));
        assert!(!f.remove(&["blocks", "b_cf0f0q3zc6vn"]));
        assert!(!f.to_string().contains("ground"));
    }
}
