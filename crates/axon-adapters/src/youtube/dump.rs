//! Prepared YouTube dump reader.
//!
//! Like the `git` adapter reads an already-materialized clone via
//! `repo_root`, this adapter reads an already-materialized YouTube
//! metadata+transcript dump via `youtube_dump_path` — a local JSON file
//! prepared by the caller (the services bridge performs the `yt-dlp`
//! subprocess fetch). Keeping the network/subprocess out of the adapter
//! makes it unit-testable with fixture dumps.
//!
//! Dump shape (one object per video):
//! ```json
//! {
//!   "videos": [
//!     {
//!       "video_id": "dQw4w9WgXcQ",
//!       "title": "Never Gonna Give You Up",
//!       "channel": "Rick Astley",
//!       "channel_url": "https://www.youtube.com/@RickAstleyYT",
//!       "uploader_id": "RickAstleyYT",
//!       "upload_date": "20091025",
//!       "description": "...",
//!       "duration_string": "3:33",
//!       "view_count": 1000000,
//!       "like_count": 10000,
//!       "tags": ["music"],
//!       "categories": ["Music"],
//!       "thumbnail": "https://...",
//!       "transcript": "Never gonna give you up..."
//!     }
//!   ]
//! }
//! ```
//!
//! Parsed by hand from `serde_json::Value` (matching the legacy
//! `axon-ingest::youtube::meta::parse_youtube_info_json` style) rather than
//! `serde_json::from_str::<T>`, since this crate does not otherwise take a
//! direct `serde` dependency.

use std::fs;
use std::path::Path;

use axon_api::source::ApiError;
use axon_error::ErrorStage;
use serde_json::Value;

use crate::adapter::Result;

/// A single video's metadata + transcript, as read from a prepared dump.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct YoutubeVideoDump {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub channel_url: String,
    pub uploader_id: String,
    pub upload_date: String,
    pub description: String,
    pub duration_string: String,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub thumbnail: String,
    pub transcript: String,
}

fn err(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        format!("adapter.youtube.{code}"),
        ErrorStage::Discovering,
        message,
    )
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

fn video_dump_from_value(value: &Value) -> YoutubeVideoDump {
    YoutubeVideoDump {
        video_id: string_field(value, "video_id"),
        title: string_field(value, "title"),
        channel: string_field(value, "channel"),
        channel_url: string_field(value, "channel_url"),
        uploader_id: string_field(value, "uploader_id"),
        upload_date: string_field(value, "upload_date"),
        description: string_field(value, "description"),
        duration_string: string_field(value, "duration_string"),
        view_count: value.get("view_count").and_then(Value::as_u64),
        like_count: value.get("like_count").and_then(Value::as_u64),
        tags: string_vec_field(value, "tags"),
        categories: string_vec_field(value, "categories"),
        thumbnail: string_field(value, "thumbnail"),
        transcript: string_field(value, "transcript"),
    }
}

/// Read and parse a prepared YouTube dump file into its video entries.
/// Malformed JSON is a hard error; a missing or empty `videos` array parses
/// successfully into an empty `Vec`.
pub fn read_youtube_dump(path: &Path) -> Result<Vec<YoutubeVideoDump>> {
    let text = fs::read_to_string(path).map_err(|e| {
        err(
            "dump.read_failed",
            format!("failed to read youtube dump at {}: {e}", path.display()),
        )
        .with_context("path", path.display().to_string())
    })?;
    let root: Value = serde_json::from_str(&text).map_err(|e| {
        err(
            "dump.invalid",
            format!("youtube dump at {} is not valid JSON: {e}", path.display()),
        )
        .with_context("path", path.display().to_string())
    })?;
    let videos_value = root.get("videos").and_then(Value::as_array);
    let Some(videos_value) = videos_value else {
        return Ok(Vec::new());
    };

    let mut videos = Vec::with_capacity(videos_value.len());
    for entry in videos_value {
        let video = video_dump_from_value(entry);
        if video.video_id.trim().is_empty() {
            return Err(err(
                "dump.video_id.missing",
                "youtube dump entry is missing a non-empty video_id",
            ));
        }
        videos.push(video);
    }
    Ok(videos)
}

#[cfg(test)]
#[path = "dump_tests.rs"]
mod tests;
