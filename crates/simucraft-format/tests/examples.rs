//! WP0.2 done-when: every example file validates, and open + save is byte-identical.

use std::path::{Path, PathBuf};

use simucraft_format::{File, Kind, check};

fn example_files() -> Vec<(PathBuf, Kind)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(kind) = Kind::from_path(&path) {
                out.push((path, kind));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_kind_has_an_example() {
    let files = example_files();
    for kind in Kind::ALL {
        assert!(files.iter().any(|(_, k)| *k == kind), "no example for {kind:?}");
    }
}

#[test]
fn examples_are_valid() {
    for (path, kind) in example_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let diagnostics = check(kind, &text);
        assert!(diagnostics.is_empty(), "{}: {diagnostics:#?}", path.display());
    }
}

#[test]
fn open_and_save_is_byte_identical() {
    for (path, kind) in example_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let file = File::parse(kind, &text).unwrap();
        assert_eq!(file.to_string(), text, "{}", path.display());
    }
}
