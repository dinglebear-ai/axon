use anyhow::{Result, bail};

use super::artifact_index::ArtifactIndex;

pub fn check_dangling_refs(index: &ArtifactIndex) -> Result<()> {
    for artifact in index.iter() {
        let Some(json) = &artifact.json else {
            continue;
        };
        collect_refs(json, &mut |reference| {
            if reference.starts_with("#/$defs/") {
                let name = reference.trim_start_matches("#/$defs/");
                if json.pointer(&format!("/$defs/{name}")).is_none() {
                    bail!(
                        "{} contains dangling local ref {reference}",
                        artifact.path.display()
                    );
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn collect_refs(
    value: &serde_json::Value,
    visit: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(|value| value.as_str()) {
                visit(reference)?;
            }
            for value in map.values() {
                collect_refs(value, visit)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_refs(value, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}
