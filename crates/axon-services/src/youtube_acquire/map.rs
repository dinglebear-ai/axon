//! Pure raw-yt-dlp → prepared-dump mapping (plus the VTT transcript parser and
//! playlist-row canonicalizer).
//!
//! No subprocess, no I/O — just structural mapping from a yt-dlp `.info.json`
//! object + a parsed transcript string into the dump-video shape the
//! `axon_adapters::youtube` adapter reads. The serialized field names here MUST
//! match [`axon_adapters::youtube::dump::YoutubeVideoDump`] (the adapter's
//! `video_dump_from_value` deserialize target) exactly, because the adapter
//! reads the file this produces. This is the highest-risk part of the youtube
//! acquisition slice, hence it is isolated and unit-tested with fixtures.
//!
//! yt-dlp's `--dump-json`/`--write-info-json` output names the video id `id`
//! (not `video_id`) and carries no transcript field, so we cannot pass its JSON
//! through untouched — [`video_dump_json`] remaps `id` → `video_id` and injects
//! the separately-fetched `transcript`.

use serde_json::{Value, json};

/// Map a yt-dlp `.info.json` object plus an already-parsed `transcript` into a
/// single dump video entry whose field names mirror
/// `axon_adapters::youtube::dump::YoutubeVideoDump`. Missing/typed-wrong fields
/// degrade to empty/`null` rather than failing — the adapter tolerates absent
/// fields (only a non-empty `video_id` is required, which yt-dlp always emits
/// as `id`).
pub(super) fn video_dump_json(info: &Value, transcript: &str) -> Value {
    json!({
        // yt-dlp emits the video id as `id`; the dump shape names it `video_id`.
        "video_id": string_field(info, "id"),
        "title": string_field(info, "title"),
        "channel": string_field(info, "channel"),
        "channel_url": string_field(info, "channel_url"),
        "uploader_id": string_field(info, "uploader_id"),
        "upload_date": string_field(info, "upload_date"),
        "description": string_field(info, "description"),
        "duration_string": string_field(info, "duration_string"),
        "view_count": info.get("view_count").and_then(Value::as_u64),
        "like_count": info.get("like_count").and_then(Value::as_u64),
        "tags": string_vec_field(info, "tags"),
        "categories": string_vec_field(info, "categories"),
        "thumbnail": string_field(info, "thumbnail"),
        "transcript": transcript,
    })
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn string_vec_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract an 11-character YouTube video id from a URL or bare id string.
/// Duplicated (intentionally) from the adapter's `extract_video_id` so the
/// playlist-row canonicalizer stays local and dependency-free; kept in sync via
/// tests.
fn extract_video_id(input: &str) -> Option<String> {
    if let Ok(url) = url::Url::parse(input) {
        let host = url.host_str().unwrap_or("");
        if host == "www.youtube.com" || host == "youtube.com" || host == "m.youtube.com" {
            for (key, value) in url.query_pairs() {
                if key == "v" {
                    return Some(value.into_owned());
                }
            }
            if let Some(id) = url.path_segments().and_then(|mut segs| {
                let first = segs.next()?;
                if matches!(first, "embed" | "shorts" | "v") {
                    segs.next().map(|s| s.to_string())
                } else {
                    None
                }
            }) && !id.is_empty()
            {
                return Some(id);
            }
            return None;
        }
        if host == "youtu.be" {
            let path = url.path().trim_start_matches('/');
            if !path.is_empty() {
                return Some(path.to_string());
            }
            return None;
        }
        return None;
    }

    let trimmed = input.trim();
    if trimmed.len() == 11
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(trimmed.to_string());
    }
    None
}

/// Canonicalize `yt-dlp --flat-playlist` rows into `watch?v=<id>` URLs, dropping
/// empty/invalid rows. Ported from the legacy
/// `axon-ingest::youtube::canonicalize_enumerated_video_rows`, minus the
/// logging (this is a pure helper).
pub(super) fn canonicalize_enumerated_video_rows(rows: Vec<String>) -> Vec<String> {
    rows.into_iter()
        .filter_map(|row| {
            let trimmed = row.trim();
            if trimmed.is_empty() {
                return None;
            }
            extract_video_id(trimmed).map(|id| format!("https://www.youtube.com/watch?v={id}"))
        })
        .collect()
}

/// Parse a WebVTT transcript string into clean plain text.
///
/// Ported from `axon-ingest::youtube::vtt::parse_vtt_to_text` (kept
/// dependency-free of the legacy crate). Strips the WEBVTT header, timestamp
/// lines, cue identifiers, NOTE/STYLE/REGION directives, HTML tags, and
/// deduplicates consecutive identical lines from overlapping subtitle windows.
pub(super) fn parse_vtt_to_text(vtt: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut last: Option<String> = None;
    let mut in_block_directive = false;
    let mut next_is_cue_body = false;

    for raw_line in vtt.lines() {
        let line = raw_line.trim_start_matches('\u{feff}');

        if line.trim().is_empty() {
            in_block_directive = false;
            next_is_cue_body = false;
            continue;
        }
        if in_block_directive {
            continue;
        }

        let trimmed = line.trim();
        if !next_is_cue_body
            && (trimmed == "WEBVTT"
                || trimmed.starts_with("WEBVTT ")
                || trimmed.starts_with("NOTE")
                || trimmed.starts_with("STYLE")
                || trimmed.starts_with("REGION"))
        {
            in_block_directive = true;
            continue;
        }

        if line.contains("-->") {
            next_is_cue_body = true;
            continue;
        }

        if !next_is_cue_body {
            continue;
        }

        let mut clean = String::new();
        let mut inside_tag = false;
        for ch in line.chars() {
            match ch {
                '<' => inside_tag = true,
                '>' => inside_tag = false,
                _ if !inside_tag => clean.push(ch),
                _ => {}
            }
        }
        let clean = clean.trim().to_string();
        if clean.is_empty() {
            continue;
        }
        if last.as_deref() == Some(&clean) {
            continue;
        }
        last = Some(clean.clone());
        result.push(clean);
    }

    result.join(" ")
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
