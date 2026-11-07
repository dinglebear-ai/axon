//! Deduplicate identical fragments within one document, retaining their locations.
use crate::chunk::DocumentChunk;
use std::collections::HashMap;

pub(super) fn deduplicate(chunks: Vec<DocumentChunk>) -> Vec<DocumentChunk> {
    let mut positions = HashMap::new();
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
        let key = serde_json::json!([
            chunk.content,
            chunk.title,
            chunk.heading_path,
            chunk.range.session_turn_id,
            chunk.range.json_pointer,
            chunk.range.yaml_path,
            chunk.range.xml_xpath,
            chunk.range.csv_row,
            chunk.range.time_start_ms,
            chunk.range.time_end_ms,
            chunk.range.turn_start,
            chunk.range.turn_end,
        ])
        .to_string();
        if let Some(&index) = positions.get(&key) {
            let existing: &mut DocumentChunk = &mut output[index];
            let mut ranges = existing
                .metadata
                .get("additional_source_ranges")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let range = serde_json::to_value(&chunk.range).unwrap();
            if chunk.range != existing.range && !ranges.contains(&range) {
                ranges.push(range);
            }
            // Bound provider metadata without losing locations: start another
            // canonical group if this group's provenance would become too large.
            if serde_json::to_vec(&ranges).unwrap().len() <= 32 * 1024 {
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
                if !ranges.is_empty() {
                    existing
                        .metadata
                        .insert("additional_source_ranges".into(), ranges.into());
                }
                continue;
            }
        }
        positions.insert(key, output.len());
        output.push(chunk);
    }
    output
}
