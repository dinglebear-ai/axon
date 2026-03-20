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
        // JSON serialization is ASCII-safe (non-ASCII is \uXXXX escaped),
        // so byte length == char count. Use .len() to avoid O(n) char scan.
        Ok(raw) if raw.len() <= max_chars => (value.clone(), false),
        Ok(raw) => {
            // Truncate at a byte boundary that is safe because the serialized
            // JSON is pure ASCII (serde_json escapes all non-ASCII).
            let clipped = &raw[..max_chars];
            (serde_json::json!({ "clipped_json": clipped }), true)
        }
        Err(_) => (
            serde_json::json!({ "clipped_json": "(serialization error)" }),
            true,
        ),
    }
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
}
