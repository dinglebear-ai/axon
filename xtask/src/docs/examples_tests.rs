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
