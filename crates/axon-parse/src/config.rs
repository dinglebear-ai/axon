//! Validation for concise structured records, owned by the parser boundary.
use axon_api::source::ContentKind;

pub const MODULE_NAME: &str = "config";

/// Only complete nonempty mappings/sequences qualify as structural evidence.
/// Scalars and malformed input retain the ordinary document quality policy.
pub fn has_record_structure(kind: ContentKind, text: &str) -> bool {
    match kind {
        ContentKind::Json | ContentKind::Structured => {
            serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| match v {
                serde_json::Value::Object(m) => !m.is_empty(),
                serde_json::Value::Array(a) => !a.is_empty(),
                _ => false,
            })
        }
        ContentKind::Yaml => {
            serde_yaml::from_str::<serde_yaml::Value>(text).is_ok_and(|v| match v {
                serde_yaml::Value::Mapping(m) => !m.is_empty(),
                serde_yaml::Value::Sequence(a) => !a.is_empty(),
                _ => false,
            })
        }
        ContentKind::Toml => toml::from_str::<toml::Value>(text)
            .is_ok_and(|v| v.as_table().is_some_and(|t| !t.is_empty())),
        _ => false,
    }
}
