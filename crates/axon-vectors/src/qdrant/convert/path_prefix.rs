//! Strict inventory prefixes with compatibility for older payloads.
use super::*;

pub(super) fn path_prefix_condition(value: &serde_json::Value) -> qdrant_client::qdrant::Condition {
    let prefix = value.as_str().unwrap_or("").trim_end_matches('/');
    let normalized = serde_json::json!(if prefix.is_empty() { "/" } else { prefix });
    let legacy = qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Filter(Filter {
            should: [
                "source_item_key",
                "source_item_aliases",
                "chunk_locator.path",
            ]
            .into_iter()
            .map(|field| text_field_condition(field, value))
            .collect(),
            must: vec![qdrant_client::qdrant::Condition {
                condition_one_of: Some(condition::ConditionOneOf::IsEmpty(IsEmptyCondition {
                    key: "source_path_prefixes".into(),
                })),
            }],
            must_not: Vec::new(),
            min_should: None,
        })),
    };
    qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Filter(Filter {
            should: vec![field_condition("source_path_prefixes", &normalized), legacy],
            must: Vec::new(),
            must_not: Vec::new(),
            min_should: None,
        })),
    }
}

fn text_field_condition(
    field: &str,
    value: &serde_json::Value,
) -> qdrant_client::qdrant::Condition {
    let text = match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    };
    qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Field(FieldCondition {
            key: field.to_string(),
            r#match: Some(Match {
                match_value: Some(r#match::MatchValue::Text(text)),
            }),
            range: None,
            geo_bounding_box: None,
            geo_radius: None,
            values_count: None,
            geo_polygon: None,
            datetime_range: None,
            is_empty: None,
            is_null: None,
        })),
    }
}
