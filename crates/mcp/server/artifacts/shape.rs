use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) fn line_count(text: &str) -> usize {
    text.lines().count()
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn clip_inline_json(value: &serde_json::Value, max_chars: usize) -> (serde_json::Value, bool) {
    match serde_json::to_string(value) {
        Ok(raw) if raw.chars().count() <= max_chars => (value.clone(), false),
        Ok(_) => match value {
            serde_json::Value::Array(arr) => clip_array(arr, max_chars),
            serde_json::Value::Object(map) => clip_object(map, max_chars),
            other => (other.clone(), false),
        },
        Err(_) => (
            serde_json::json!({"__error__": "serialization failed"}),
            true,
        ),
    }
}

fn clip_array(arr: &[serde_json::Value], max_chars: usize) -> (serde_json::Value, bool) {
    let budget = max_chars.saturating_sub(30);
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut used = 2usize; // "[]"
    for item in arr {
        let s = serde_json::to_string(item).unwrap_or_default();
        let cost = s.chars().count() + if out.is_empty() { 0 } else { 1 };
        if used + cost > budget {
            break;
        }
        out.push(item.clone());
        used += cost;
    }
    let remaining = arr.len() - out.len();
    if remaining > 0 {
        out.push(serde_json::json!({"__truncated__": remaining}));
        (serde_json::Value::Array(out), true)
    } else {
        (serde_json::Value::Array(out), false)
    }
}

fn clip_object(
    map: &serde_json::Map<String, serde_json::Value>,
    max_chars: usize,
) -> (serde_json::Value, bool) {
    let string_cap = (max_chars / 4).max(200);
    let mut truncated = false;
    let out: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, v)| {
            let v2 = match v {
                serde_json::Value::String(s) if s.chars().count() > string_cap => {
                    truncated = true;
                    let head: String = s.chars().take(string_cap).collect();
                    serde_json::json!({
                        "__head__": head,
                        "__total_chars__": s.chars().count(),
                    })
                }
                other => other.clone(),
            };
            (k.clone(), v2)
        })
        .collect();
    (serde_json::Value::Object(out), truncated)
}

/// For arrays of objects, compute a status histogram over common status-like fields.
/// Returns `None` if the array is empty or no object has a recognized field.
fn status_histogram(arr: &[serde_json::Value]) -> Option<serde_json::Value> {
    const STATUS_KEYS: &[&str] = &["status", "phase", "state"];
    if arr.is_empty() {
        return None;
    }
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut found = false;
    for item in arr {
        if let serde_json::Value::Object(obj) = item {
            for key in STATUS_KEYS {
                if let Some(serde_json::Value::String(s)) = obj.get(*key) {
                    *counts.entry(s.clone()).or_insert(0) += 1;
                    found = true;
                    break;
                }
            }
        }
    }
    found.then(|| serde_json::to_value(counts).unwrap_or_default())
}

/// Recursive shape summary for path-mode responses.
/// Objects: key -> shape of value.
/// Arrays with a status-like field: `{"total": N, "by_status": {...}}`.
/// Arrays without: `"<array[N]>"`.
/// Strings <= 100 chars: verbatim. Longer strings: `"<string N>"`.
/// Primitives: verbatim.
pub fn json_shape_preview(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), json_shape_preview(v)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        serde_json::Value::Array(arr) => match status_histogram(arr) {
            Some(hist) => serde_json::json!({ "total": arr.len(), "by_status": hist }),
            None => format!("<array[{}]>", arr.len()).into(),
        },
        serde_json::Value::String(s) if s.chars().count() <= 100 => {
            serde_json::Value::String(s.clone())
        }
        serde_json::Value::String(s) => format!("<string {}>", s.chars().count()).into(),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_preview_short_strings_are_verbatim() {
        let val = serde_json::json!({
            "name": "axon",
            "count": 42,
            "items": [1, 2, 3],
            "nested": { "key": "value" },
        });
        let preview = json_shape_preview(&val);
        assert_eq!(preview["name"], "axon");
        assert_eq!(preview["count"], 42);
        assert_eq!(preview["items"], "<array[3]>");
        assert!(preview["nested"].is_object());
        assert_eq!(preview["nested"]["key"], "value");
    }

    #[test]
    fn json_shape_preview_long_strings_are_summarized() {
        let long = "x".repeat(101);
        let val = serde_json::json!({ "body": long });
        let preview = json_shape_preview(&val);
        assert_eq!(preview["body"], "<string 101>");
    }

    #[test]
    fn json_shape_preview_status_histogram() {
        let val = serde_json::json!({
            "jobs": [
                {"id": 1, "status": "completed"},
                {"id": 2, "status": "running"},
                {"id": 3, "status": "completed"},
                {"id": 4, "status": "failed"},
            ]
        });
        let preview = json_shape_preview(&val);
        let jobs = &preview["jobs"];
        assert_eq!(jobs["total"], 4);
        assert_eq!(jobs["by_status"]["completed"], 2);
        assert_eq!(jobs["by_status"]["running"], 1);
        assert_eq!(jobs["by_status"]["failed"], 1);
    }

    #[test]
    fn status_histogram_returns_none_for_non_object_arrays() {
        let arr = vec![
            serde_json::json!(1),
            serde_json::json!(2),
            serde_json::json!(3),
        ];
        assert!(status_histogram(&arr).is_none());
    }

    #[test]
    fn clip_inline_json_array_truncates_at_item_boundaries() {
        let items: Vec<_> = (0..5)
            .map(|i| serde_json::json!({"id": i, "text": "x".repeat(200)}))
            .collect();
        let val = serde_json::Value::Array(items);
        let (clipped, truncated) = clip_inline_json(&val, 600);
        assert!(truncated, "should be truncated");
        let arr = clipped.as_array().expect("must be array");
        let last = arr.last().expect("must have items");
        assert!(
            last.get("__truncated__").is_some(),
            "must have truncation marker"
        );
        for item in &arr[..arr.len() - 1] {
            assert!(item.get("id").is_some(), "item must be complete object");
        }
    }

    #[test]
    fn clip_inline_json_object_truncates_long_string_fields() {
        let long_val = "x".repeat(600);
        let val = serde_json::json!({
            "query": "short",
            "answer": long_val,
            "count": 42,
        });
        let (clipped, truncated) = clip_inline_json(&val, 300);
        assert!(truncated, "should be truncated");
        assert!(clipped.get("query").is_some());
        assert!(clipped.get("answer").is_some());
        assert!(clipped.get("count").is_some());
        let answer = &clipped["answer"];
        assert!(answer.is_object(), "long string must become head object");
        assert!(answer.get("__head__").is_some(), "must have __head__ field");
        assert!(
            answer.get("__total_chars__").is_some(),
            "must have __total_chars__"
        );
        assert_eq!(clipped["query"], "short");
        assert_eq!(clipped["count"], 42);
    }

    #[test]
    fn clip_inline_json_does_not_produce_clipped_json_wrapper() {
        let large_obj = serde_json::json!({
            "a": "x".repeat(5000),
            "b": "y".repeat(5000),
            "c": "z".repeat(5000),
        });
        let (clipped, _) = clip_inline_json(&large_obj, 100);
        let serialized = serde_json::to_string(&clipped).unwrap();
        assert!(
            !serialized.contains("clipped_json"),
            "must not produce clipped_json wrapper"
        );
    }

    #[test]
    fn clip_inline_json_small_payload_is_unchanged() {
        let val = serde_json::json!({"key": "value", "n": 42});
        let (clipped, truncated) = clip_inline_json(&val, 10_000);
        assert!(!truncated);
        assert_eq!(clipped, val);
    }
}
