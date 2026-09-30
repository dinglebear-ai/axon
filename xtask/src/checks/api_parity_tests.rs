use super::*;
use serde_json::json;

fn put(root: &Path, path: &str, value: Value) {
    let target = root.join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, value.to_string()).unwrap();
}

fn fixture(root: &Path) {
    put(
        root,
        "docs/reference/cli/commands.json",
        json!({"commands":[{"path":["code-search"]},{"path":["crawl"]}]}),
    );
    put(
        root,
        "docs/reference/mcp/tool-schema.json",
        json!({"x-axon":{"operations":[{"action":"code_search"},{"action":"artifacts"},{"action":"watch"}]}}),
    );
    put(
        root,
        "docs/reference/rest/openapi.json",
        json!({"routes":[{"path":"/v1/code-search"},{"path":"/v1/artifacts/{id}"}]}),
    );
}

#[test]
fn parity_uses_full_canonical_catalog_and_normalizes_only_spelling() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let text = render(root.path()).unwrap();
    assert!(text.contains("| code_search | yes | yes | yes |"));
    assert!(text.contains("| artifacts | no | yes | yes |"));
    assert!(text.contains("| crawl | yes | no | no |"));
    assert!(text.contains("not proof"));
}

#[test]
fn parity_check_rejects_drift_without_writing() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    write(root.path()).unwrap();
    check(root.path()).unwrap();
    let path = root.path().join(OUTPUT);
    std::fs::write(&path, "hand edit\n").unwrap();
    assert!(check(root.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "hand edit\n");
}

#[test]
fn parity_rejects_missing_or_empty_registry_instead_of_empty_success() {
    let root = tempfile::tempdir().unwrap();
    assert!(render(root.path()).is_err());
    fixture(root.path());
    put(
        root.path(),
        "docs/reference/mcp/tool-schema.json",
        json!({"x-axon":{"operations":[]}}),
    );
    assert!(
        render(root.path())
            .unwrap_err()
            .to_string()
            .contains("nonempty")
    );
}
