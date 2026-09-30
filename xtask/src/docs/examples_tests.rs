use super::*;
use std::fs;

const WIDGET_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": { "name": { "type": "string" } },
  "required": ["name"],
  "additionalProperties": false
}"#;

fn write(root: &Path, rel_path: &str, content: &str) {
    let path = root.join(rel_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn passes_with_no_docs_reference_tree() {
    let dir = tempfile::tempdir().unwrap();
    check(dir.path()).unwrap();
}

#[test]
fn passes_when_no_examples_are_marked() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.md",
        "# Widget\n\nNo markers here.\n",
    );
    check(dir.path()).unwrap();
}

#[test]
fn ignores_unmarked_fences_even_when_invalid() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.md",
        "# Widget\n\n```json\nnot json at all\n```\n",
    );
    check(dir.path()).unwrap();
}

#[test]
fn validates_a_passing_marked_json_example() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "# Widget\n\n<!-- doc-example: kind=json schema=widget.schema.json -->\n```json\n{\"name\": \"widget\"}\n```\n",
    );
    check(dir.path()).unwrap();
}

#[test]
fn rejects_a_marked_json_example_that_fails_schema_validation() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "# Widget\n\n<!-- doc-example: kind=json schema=widget.schema.json -->\n```json\n{\"name\": 123}\n```\n",
    );
    let err = check(dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("widget.md"), "{msg}");
    assert!(msg.contains("line 3"), "{msg}");
    assert!(msg.contains("failed schema validation"), "{msg}");
}

#[test]
fn validates_a_passing_marked_toml_example_after_json_conversion() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=toml schema=widget.schema.json -->\n```toml\nname = \"widget\"\n```\n",
    );
    check(dir.path()).unwrap();
}

#[test]
fn rejects_invalid_json_body() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=json schema=widget.schema.json -->\n```json\n{not valid json\n```\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("invalid JSON"));
}

#[test]
fn rejects_when_schema_file_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=json schema=missing.schema.json -->\n```json\n{\"name\": \"widget\"}\n```\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("not found under docs/reference"));
}

#[test]
fn rejects_when_marker_is_missing_kind() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: schema=widget.schema.json -->\n```json\n{\"name\": \"widget\"}\n```\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("missing `kind=`"));
}

#[test]
fn rejects_when_marker_kind_does_not_match_fence_language() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=toml schema=widget.schema.json -->\n```json\n{\"name\": \"widget\"}\n```\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("does not match fence language"));
}

#[test]
fn rejects_when_marker_has_no_following_fence() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=json schema=widget.schema.json -->\nno fence here at all\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(
        err.to_string()
            .contains("not immediately followed by a fenced code block")
    );
}

#[test]
fn rejects_unterminated_fence() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        "<!-- doc-example: kind=json schema=widget.schema.json -->\n```json\n{\"name\": \"widget\"}\n",
    );
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("unterminated"));
}

#[test]
fn validates_multiple_examples_against_the_same_cached_schema() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/widget.schema.json",
        WIDGET_SCHEMA,
    );
    write(
        dir.path(),
        "docs/reference/widget.md",
        concat!(
            "<!-- doc-example: kind=json schema=widget.schema.json -->\n",
            "```json\n{\"name\": \"one\"}\n```\n\n",
            "<!-- doc-example: kind=json schema=widget.schema.json -->\n",
            "```json\n{\"name\": \"two\"}\n```\n",
        ),
    );
    check(dir.path()).unwrap();
}

#[test]
fn parse_markers_reports_one_based_marker_line() {
    let content = "line one\nline two\n<!-- doc-example: kind=json schema=widget.schema.json -->\n```json\n{}\n```\n";
    let found = parse_markers(content, "widget.md");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].marker_line, 3);
    assert_eq!(found[0].body.as_deref(), Ok("{}"));
}

#[test]
fn bundle_definition_keeps_transitive_refs_without_bundle_root_constraints() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/bundle.json",
        r##"{
      "$schema":"https://json-schema.org/draft/2020-12/schema",
      "type":"object","additionalProperties":false,
      "$defs":{"Name":{"type":"string"},"Input":{"type":"object","properties":{"name":{"$ref":"#/$defs/Name"}},"required":["name"],"additionalProperties":false}}
    }"##,
    );
    let root = dir.path().join("docs/reference");
    let selected = schema_loader::load(&root, "bundle.json#/$defs/Input").unwrap();
    let validator = jsonschema::validator_for(&selected).unwrap();
    assert!(validator.is_valid(&serde_json::json!({"name":"valid"})));
    assert!(!validator.is_valid(&serde_json::json!({"name":42})));
    assert!(!validator.is_valid(&serde_json::json!({"name":"valid","extra":true})));
    let mut cache = HashMap::new();
    get_or_build_validator(&root, "bundle.json", &mut cache).unwrap();
    get_or_build_validator(&root, "bundle.json#/$defs/Input", &mut cache).unwrap();
    assert_eq!(cache.len(), 2);
}

#[test]
fn schema_definition_and_path_errors_are_actionable() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "docs/reference/bundle.json",
        r#"{"$defs":{"Known":{"type":"string"}}}"#,
    );
    let root = dir.path().join("docs/reference");
    for reference in [
        "bundle.json#/$defs/Unknown",
        "bundle.json#/properties/name",
        "../outside.json",
        "/tmp/outside.json",
    ] {
        assert!(
            schema_loader::load(&root, reference).is_err(),
            "{reference}"
        );
    }
}

#[test]
#[cfg(unix)]
fn schema_symlink_cannot_escape_reference_tree() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "outside.json", WIDGET_SCHEMA);
    let root = dir.path().join("docs/reference");
    fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.json"), root.join("escape.json")).unwrap();
    let err = schema_loader::load(&root, "escape.json").unwrap_err();
    assert!(err.contains("outside docs/reference"), "{err}");
}
