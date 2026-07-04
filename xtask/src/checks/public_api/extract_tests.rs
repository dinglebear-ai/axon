use super::*;

use std::fs;
use std::path::Path;

/// Build a throwaway crate dir with the given `src/<rel>` files and return its path.
fn crate_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let path = dir.path().join("src").join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    dir
}

fn paths(dir: &Path) -> Vec<String> {
    extract_crate(dir)
        .unwrap()
        .into_iter()
        .map(|e| format!("{} ({})", e.path, e.kind))
        .collect()
}

#[test]
fn records_public_items_and_excludes_restricted_and_private() {
    let dir = crate_with(&[(
        "lib.rs",
        "pub fn open() {}\n\
         pub struct Doc;\n\
         pub enum Kind { A }\n\
         pub const N: u8 = 1;\n\
         pub(crate) fn hidden() {}\n\
         pub(super) fn also_hidden() {}\n\
         fn private() {}\n",
    )]);
    let got = paths(dir.path());
    assert!(got.contains(&"open (fn)".to_string()));
    assert!(got.contains(&"Doc (struct)".to_string()));
    assert!(got.contains(&"Kind (enum)".to_string()));
    assert!(got.contains(&"N (const)".to_string()));
    assert!(!got.iter().any(|g| g.starts_with("hidden")));
    assert!(!got.iter().any(|g| g.starts_with("also_hidden")));
    assert!(!got.iter().any(|g| g.starts_with("private")));
}

#[test]
fn recurses_into_public_file_module_but_not_private_module() {
    let dir = crate_with(&[
        ("lib.rs", "pub mod open;\nmod closed;\n"),
        ("open.rs", "pub fn f() {}\n"),
        ("closed.rs", "pub fn g() {}\n"),
    ]);
    let got = paths(dir.path());
    assert!(got.contains(&"open (mod)".to_string()));
    assert!(got.contains(&"open::f (fn)".to_string()));
    // `mod closed;` is not `pub`, so nothing under it is crate-public.
    assert!(!got.iter().any(|g| g.contains("closed")));
    assert!(!got.iter().any(|g| g.contains("::g")));
}

#[test]
fn recurses_into_nested_and_inline_public_modules() {
    let dir = crate_with(&[
        ("lib.rs", "pub mod a;\npub mod inl { pub fn z() {} }\n"),
        ("a.rs", "pub mod b;\npub fn top() {}\n"),
        ("a/b.rs", "pub struct Deep;\n"),
    ]);
    let got = paths(dir.path());
    assert!(got.contains(&"a::top (fn)".to_string()));
    assert!(got.contains(&"a::b::Deep (struct)".to_string()));
    assert!(got.contains(&"inl::z (fn)".to_string()));
}

#[test]
fn records_pub_use_reexport_leaves_with_rename_and_group() {
    let dir = crate_with(&[(
        "lib.rs",
        "pub use crate::inner::Thing;\n\
         pub use crate::inner::Other as Renamed;\n\
         pub use crate::inner::{One, Two};\n\
         pub use crate::inner::*;\n",
    )]);
    let got = paths(dir.path());
    assert!(got.contains(&"Thing (use)".to_string()));
    assert!(got.contains(&"Renamed (use)".to_string()));
    assert!(got.contains(&"One (use)".to_string()));
    assert!(got.contains(&"Two (use)".to_string()));
    assert!(got.contains(&"* (use)".to_string()));
}

#[test]
fn skips_cfg_test_modules() {
    let dir = crate_with(&[(
        "lib.rs",
        "pub fn real() {}\n\
         #[cfg(test)]\n\
         pub mod tests { pub fn helper() {} }\n",
    )]);
    let got = paths(dir.path());
    assert!(got.contains(&"real (fn)".to_string()));
    assert!(!got.iter().any(|g| g.contains("tests")));
    assert!(!got.iter().any(|g| g.contains("helper")));
}

#[test]
fn binary_only_crate_has_empty_surface() {
    let dir = crate_with(&[("main.rs", "fn main() {}\n")]);
    assert!(extract_crate(dir.path()).unwrap().is_empty());
}

#[test]
fn output_is_sorted_and_deduplicated() {
    let dir = crate_with(&[("lib.rs", "pub fn b() {}\npub fn a() {}\n")]);
    let got = paths(dir.path());
    let sorted = {
        let mut s = got.clone();
        s.sort();
        s
    };
    assert_eq!(got, sorted);
}
