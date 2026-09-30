//! Resolve an example schema or a definition inside a generated bundle.
use serde_json::{Value, json};
use std::path::{Component, Path};

pub(super) fn load(root: &Path, reference: &str) -> Result<Value, String> {
    let (name, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    let relative = Path::new(name);
    if name.is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "schema {reference:?} must be a relative path under docs/reference"
        ));
    }
    let base = root
        .canonicalize()
        .map_err(|e| format!("resolve schema root: {e}"))?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("schema {reference:?} not found under docs/reference: {e}"))?;
    if !path.starts_with(&base) {
        return Err(format!(
            "schema {reference:?} resolves outside docs/reference; use an owned schema"
        ));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("read schema {reference:?}: {e}"))?;
    let schema: Value = serde_json::from_str(&content)
        .map_err(|e| format!("schema {reference:?} is not valid JSON: {e}"))?;
    if fragment.is_empty() {
        return Ok(schema);
    }
    if !fragment.starts_with("/$defs/") || schema.pointer(fragment).is_none() {
        return Err(format!(
            "schema {reference:?} selects a missing or unsupported definition; use #/$defs/<name> from the generated bundle"
        ));
    }
    // Keep the definition namespace for transitive local refs, without the
    // non-request constraints at the root of a schema bundle.
    let mut selected = json!({"$defs": schema["$defs"], "$ref": format!("#{fragment}")});
    if let Some(dialect) = schema.get("$schema") {
        selected["$schema"] = dialect.clone();
    }
    Ok(selected)
}
