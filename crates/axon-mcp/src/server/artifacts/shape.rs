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
/// Arrays without: `{"total": N, "sample": [first 2 items shape-previewed]}`.
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
            None => {
                let sample: Vec<_> = arr.iter().take(2).map(json_shape_preview).collect();
                serde_json::json!({ "total": arr.len(), "sample": sample })
            }
        },
        serde_json::Value::String(s) if s.chars().count() <= 100 => {
            serde_json::Value::String(s.clone())
        }
        serde_json::Value::String(s) => format!("<string {}>", s.chars().count()).into(),
        other => other.clone(),
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
