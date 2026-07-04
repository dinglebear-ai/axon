//! YouTube target parsing — classifies a YouTube URL (or bare video ID) as a
//! single video or a playlist/channel, and extracts the video ID when
//! applicable. Ported from the legacy `axon-ingest::youtube` parser, minus
//! the `yt-dlp` subprocess coupling — this module is pure string/URL logic.

use axon_api::source::{ApiError, SourceScope};
use axon_error::ErrorStage;
use url::Url;

use crate::adapter::Result;

/// A parsed YouTube target: either a single video (with its extracted
/// video ID) or a playlist/channel handle (scope-only; enumeration is the
/// bridge's job, mirroring how `repo_root` is a prepared filesystem input
/// for the git adapter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeTarget {
    pub scope: SourceScope,
    pub video_id: Option<String>,
    pub canonical_uri: String,
}

fn err(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        format!("adapter.youtube.{code}"),
        ErrorStage::Planning,
        message,
    )
}

/// Extract an 11-character YouTube video ID from a URL or bare ID string.
/// Mirrors `axon-ingest::youtube::extract_video_id`.
pub fn extract_video_id(input: &str) -> Option<String> {
    if let Ok(url) = Url::parse(input) {
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

/// True when the URL identifies a playlist or channel rather than a single
/// video. Mirrors `axon-ingest::youtube::is_playlist_or_channel_url`.
pub fn is_playlist_or_channel_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    if !matches!(host, "www.youtube.com" | "youtube.com" | "m.youtube.com") {
        return false;
    }
    if parsed.query_pairs().any(|(k, _)| k == "list")
        && !parsed.query_pairs().any(|(k, _)| k == "v")
    {
        return true;
    }
    if let Some(first_seg) = parsed.path_segments().and_then(|mut s| s.next()) {
        if matches!(first_seg, "c" | "channel" | "user") {
            return true;
        }
        if first_seg.starts_with('@') {
            return true;
        }
    }
    false
}

fn normalize_target(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed.starts_with('@') {
        format!("https://www.youtube.com/{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Parse a raw target (URL or bare video ID) into a `YoutubeTarget`,
/// classifying it as a video or a channel/playlist.
pub fn parse_youtube_target(input: &str) -> Result<YoutubeTarget> {
    let normalized = normalize_target(input);
    if is_playlist_or_channel_url(&normalized) {
        return Ok(YoutubeTarget {
            scope: SourceScope::Channel,
            video_id: None,
            canonical_uri: normalized,
        });
    }
    if let Some(video_id) = extract_video_id(&normalized) {
        let canonical_uri = format!("https://www.youtube.com/watch?v={video_id}");
        return Ok(YoutubeTarget {
            scope: SourceScope::Video,
            video_id: Some(video_id),
            canonical_uri,
        });
    }
    Err(err(
        "target.invalid",
        "target does not appear to be a YouTube video, playlist, or channel",
    ))
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
