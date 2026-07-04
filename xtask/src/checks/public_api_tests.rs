use super::*;

use std::path::Path;

fn entry(path: &str, kind: &'static str) -> ApiEntry {
    ApiEntry {
        path: path.to_string(),
        kind,
    }
}

/// Repo root = xtask's parent directory.
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn render_is_deterministic_and_groups_by_crate() {
    let mut surface: Surface = BTreeMap::new();
    surface.insert(
        "axon-b".to_string(),
        vec![entry("Thing", "struct"), entry("go", "fn")],
    );
    surface.insert("axon-a".to_string(), vec![]);
    let out = render(&surface);
    // Header present.
    assert!(out.starts_with("# Public API Surface"));
    // Crates rendered in sorted (BTreeMap) order: axon-a before axon-b.
    let a = out.find("## axon-a").unwrap();
    let b = out.find("## axon-b").unwrap();
    assert!(a < b);
    // Empty crate gets the placeholder line.
    assert!(out.contains("_(no crate-public library surface)_"));
    // Item formatting.
    assert!(out.contains("- `Thing` (struct)"));
    assert!(out.contains("- `go` (fn)"));
    // Same input renders identically.
    assert_eq!(out, render(&surface));
}

#[test]
fn build_covers_real_workspace_crates() {
    let surface = build(repo_root()).unwrap();
    // A representative low-level crate with a known public surface.
    let err = surface
        .get("axon-error")
        .expect("axon-error present in workspace surface");
    let has = |name: &str, kind: &str| err.iter().any(|e| e.path == name && e.kind == kind);
    assert!(has("CRATE_NAME", "const"), "axon-error::CRATE_NAME const");
    assert!(
        err.iter().any(|e| e.path == "ApiError" && e.kind == "use"),
        "axon-error re-exports ApiError"
    );
    // Sanity: many crates, and the surface is non-trivial.
    assert!(surface.len() >= 20, "expected the full crate set");
}
