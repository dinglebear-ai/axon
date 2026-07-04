use super::*;

use std::fs;

fn write_crate(root: &Path, name: &str, deps: &[&str]) {
    let dir = root.join("crates").join(name);
    fs::create_dir_all(&dir).unwrap();
    let mut toml = format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\n\n[dependencies]\n");
    for d in deps {
        toml.push_str(&format!("{d} = {{ path = \"../{d}\" }}\n"));
    }
    fs::write(dir.join("Cargo.toml"), toml).unwrap();
}

#[test]
fn build_graph_collects_workspace_deps_only() {
    let root = tempfile::tempdir().unwrap();
    write_crate(root.path(), "axon-core", &[]);
    write_crate(root.path(), "axon-api", &[]);
    // axon-core depends on axon-api (+ a non-workspace dep that must be ignored)
    fs::write(
        root.path().join("crates/axon-core/Cargo.toml"),
        "[package]\nname = \"axon-core\"\nversion = \"0\"\n\n[dependencies]\naxon-api = { path = \"../axon-api\" }\nserde = \"1\"\n",
    )
    .unwrap();
    let g = build_graph(root.path()).unwrap();
    assert_eq!(
        g.get("axon-core").unwrap().iter().collect::<Vec<_>>(),
        vec!["axon-api"]
    );
    assert!(g.get("axon-api").unwrap().is_empty());
}

#[test]
fn check_acyclic_passes_on_dag() {
    let root = tempfile::tempdir().unwrap();
    write_crate(root.path(), "axon-api", &[]);
    write_crate(root.path(), "axon-core", &["axon-api"]);
    write_crate(root.path(), "axon-services", &["axon-core"]);
    let g = build_graph(root.path()).unwrap();
    check_acyclic(&g).expect("a DAG is acyclic");
}

#[test]
fn check_acyclic_detects_cycle() {
    let root = tempfile::tempdir().unwrap();
    write_crate(root.path(), "axon-a", &["axon-b"]);
    write_crate(root.path(), "axon-b", &["axon-a"]);
    let g = build_graph(root.path()).unwrap();
    let err = check_acyclic(&g).expect_err("a→b→a is a cycle");
    assert!(err.to_string().contains("cycle"));
}

#[test]
fn check_fails_on_snapshot_drift_then_passes_after_write() {
    let root = tempfile::tempdir().unwrap();
    write_crate(root.path(), "axon-api", &[]);
    write_crate(root.path(), "axon-core", &["axon-api"]);
    // No snapshot yet -> drift.
    assert!(check(root.path()).is_err());
    write(root.path()).unwrap();
    // Now in sync.
    check(root.path()).expect("snapshot matches after write");
    // Mutating the graph without regenerating -> drift again.
    write_crate(root.path(), "axon-new", &["axon-core"]);
    assert!(check(root.path()).is_err());
}
