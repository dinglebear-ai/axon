//! Deduplicate identical fragments within one document, retaining their locations.
use crate::chunk::DocumentChunk;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

pub(super) fn deduplicate(chunks: Vec<DocumentChunk>) -> Vec<DocumentChunk> {
    let mut positions: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut provenance_bytes = HashMap::<usize, usize>::new();
    let mut output: Vec<DocumentChunk> = Vec::new();
    for chunk in chunks {
        if chunk.range.session_turn_id.is_some()
            || chunk.range.json_pointer.is_some()
            || chunk.range.yaml_path.is_some()
            || chunk.range.xml_xpath.is_some()
            || chunk.range.csv_row.is_some()
            || chunk.range.time_start_ms.is_some()
            || chunk.range.turn_start.is_some()
        {
            output.push(chunk);
            continue;
        }
        // Record boundaries matter even for identical text: one session turn or
        // structured field cannot silently stand in for a different one.
        let mut hasher = DefaultHasher::new();
        // All record identity fields checked above are None here. The remaining
        // fields preserve the former JSON key without copying chunk text.
        identity(&chunk).hash(&mut hasher);
        let key = hasher.finish();
        let matching = positions.get(&key).and_then(|bucket| {
            bucket
                .iter()
                .position(|&index| identity(&output[index]) == identity(&chunk))
        });
        if let Some(slot) = matching {
            let index = positions[&key][slot];
            let existing = &mut output[index];
            let range = serde_json::to_value(&chunk.range).unwrap();
            let ranges = existing
                .metadata
                .get("additional_source_ranges")
                .and_then(|value| value.as_array());
            let append = chunk.range != existing.range
                && !ranges.is_some_and(|ranges| ranges.contains(&range));
            let count = ranges.map_or(0, Vec::len);
            let bytes = provenance_bytes.entry(index).or_insert_with(|| {
                ranges.map_or(2, |ranges| serde_json::to_vec(ranges).unwrap().len())
            });
            let next_bytes = if append {
                *bytes + serde_json::to_vec(&range).unwrap().len() + usize::from(count > 0)
            } else {
                *bytes
            };
            // Bound provenance exactly as before, without cloning or encoding
            // the entire growing array for every duplicate.
            if next_bytes <= 32 * 1024 {
                *bytes = next_bytes;
                super::remember_symbols(existing, &chunk);
                if chunk
                    .metadata
                    .get("code_syntax_recovered")
                    .and_then(|v| v.as_bool())
                    == Some(true)
                {
                    existing
                        .metadata
                        .insert("code_syntax_recovered".into(), true.into());
                }
                if chunk
                    .metadata
                    .get("code_parse_status")
                    .and_then(|v| v.as_str())
                    == Some("partial")
                {
                    existing
                        .metadata
                        .insert("code_parse_status".into(), "partial".into());
                }
                if existing.metadata.get("actual_chunking_method")
                    != chunk.metadata.get("actual_chunking_method")
                {
                    existing.metadata.insert(
                        "actual_chunking_method".into(),
                        "source_adjacent_packing".into(),
                    );
                }
                if append {
                    let value = existing
                        .metadata
                        .entry("additional_source_ranges".into())
                        .or_insert_with(|| serde_json::json!([]));
                    if !value.is_array() {
                        *value = serde_json::json!([]);
                    }
                    value.as_array_mut().unwrap().push(range);
                }
                continue;
            }
            // A full group yields to a new canonical group of the same identity.
            positions.get_mut(&key).unwrap()[slot] = output.len();
        } else {
            positions.entry(key).or_default().push(output.len());
        }
        output.push(chunk);
    }
    output
}

fn identity(
    chunk: &DocumentChunk,
) -> (
    &str,
    &Option<String>,
    &[String],
    Option<u64>,
    &Option<String>,
) {
    (
        &chunk.content,
        &chunk.title,
        &chunk.heading_path,
        chunk.range.time_end_ms,
        &chunk.range.turn_end,
    )
}
