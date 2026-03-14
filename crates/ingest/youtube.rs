mod meta;
mod vtt;

pub use vtt::parse_vtt_to_text;

use crate::crates::core::config::Config;
use crate::crates::core::http::validate_url;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};
use spider::url::Url;
use std::error::Error;

/// Extract a YouTube video ID from a URL or return the string as-is if already an ID.
pub fn extract_video_id(input: &str) -> Option<String> {
    // Try parsing as a URL first
    if let Ok(url) = Url::parse(input) {
        let host = url.host_str().unwrap_or("");

        // https://www.youtube.com/watch?v=<ID> (also m.youtube.com)
        if host == "www.youtube.com" || host == "youtube.com" || host == "m.youtube.com" {
            for (key, value) in url.query_pairs() {
                if key == "v" {
                    return Some(value.into_owned());
                }
            }
            // Handle /embed/<ID>, /shorts/<ID>, /v/<ID> path patterns
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

        // https://youtu.be/<ID>
        if host == "youtu.be" {
            let path = url.path().trim_start_matches('/');
            if !path.is_empty() {
                return Some(path.to_string());
            }
            return None;
        }

        return None;
    }

    // Not a URL — check if it's a bare 11-character video ID
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

/// Returns `true` if `url` is a YouTube playlist or channel URL rather than a single video.
///
/// Handles:
/// - `youtube.com/playlist?list=...` (without a `?v=` param)
/// - `youtube.com/@handle`
/// - `youtube.com/c/ChannelName`
/// - `youtube.com/channel/UCxxx`
/// - `youtube.com/user/username`
pub fn is_playlist_or_channel_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    if !matches!(host, "www.youtube.com" | "youtube.com" | "m.youtube.com") {
        return false;
    }
    // playlist?list=... without a ?v= single-video param
    if parsed.query_pairs().any(|(k, _)| k == "list")
        && !parsed.query_pairs().any(|(k, _)| k == "v")
    {
        return true;
    }
    // Channel paths: /c/, /channel/, /user/, /@handle
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

/// Run `yt-dlp --flat-playlist` to enumerate all individual video URLs in a playlist or channel.
pub async fn enumerate_playlist_videos(url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    // SSRF guard: validate before invoking yt-dlp so malicious targets cannot make
    // yt-dlp fetch internal/private network resources.
    validate_url(url)?;

    let output = tokio::process::Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--print",
            "%(url)s",
            "--no-exec",
            "--",
            url,
        ])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found or failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp --flat-playlist exited non-zero: {stderr}").into());
    }

    let urls: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect();

    Ok(urls)
}

/// Invoke `yt-dlp` to download English VTT subtitles + `.info.json` for `safe_url` into
/// `tmp_path`. The URL must already be sanitized (validated video ID form) — `"--"` prevents
/// further argument injection. Returns `Err` if yt-dlp is missing or exits non-zero.
async fn run_ytdlp(safe_url: &str, tmp_path: &str) -> Result<(), Box<dyn Error>> {
    // --write-info-json writes <id>.info.json (title, channel, tags, description, etc.)
    // --no-exec prevents execution of post-processing commands.
    // "--" separates flags from the URL argument to prevent argument injection.
    let output = tokio::process::Command::new("yt-dlp")
        .args([
            "--write-auto-sub",
            "--write-info-json",
            "--skip-download",
            "--sub-format",
            "vtt",
            "--convert-subs",
            "vtt",
            "--sub-langs",
            "en",
            "--no-exec",
            "--no-warnings",
            "--sleep-requests",
            "1",
            "-o",
            &format!("{tmp_path}/%(id)s"),
            "--",
            safe_url,
        ])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found or failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp exited non-zero: {stderr}").into());
    }
    Ok(())
}

/// Resolve a user-supplied YouTube target (URL, short URL, or bare video ID)
/// to a canonical `(video_id, safe_url)` pair.
///
/// Returns an error if the input cannot be parsed as a valid YouTube video.
/// The returned `safe_url` is always in `https://www.youtube.com/watch?v=ID`
/// form, safe to pass to yt-dlp without argument-injection risk.
fn resolve_video_id_and_safe_url(url: &str) -> Result<(String, String), Box<dyn Error>> {
    let video_id = extract_video_id(url).ok_or("URL does not appear to be a YouTube video URL")?;
    let safe_url = format!("https://www.youtube.com/watch?v={video_id}");
    Ok((video_id, safe_url))
}

/// Ingest a YouTube video URL (or bare video ID) by:
/// 1. Running yt-dlp to download VTT subtitle files into a temp directory
/// 2. Parsing each VTT file into clean text via parse_vtt_to_text
/// 3. Embedding each transcript into Qdrant via the PreparedDoc pipeline
///
/// Requires `yt-dlp` to be installed and on PATH.
pub async fn ingest_youtube(cfg: &Config, url: &str) -> Result<usize, Box<dyn Error>> {
    log_info(&format!("command=ingest source=youtube target={url}"));
    // Extract and validate YouTube video ID to prevent argument injection.
    // This must happen before the SSRF check because bare 11-character video IDs
    // (e.g. "dQw4w9WgXcQ") are not URLs and would fail validate_url before
    // canonicalization. We validate the canonicalized safe_url instead.
    let (video_id, safe_url) = resolve_video_id_and_safe_url(url)?;

    // SSRF guard: validate the canonicalized URL against private IP ranges.
    // YouTube.com is always a public host, so this is belt-and-suspenders against
    // any unexpected bypass in extract_video_id.
    validate_url(&safe_url)?;

    // Create a temp directory; cleaned up automatically when `tmp` is dropped
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();

    run_ytdlp(&safe_url, &tmp_path).await?;

    // Collect .vtt and .info.json files produced by yt-dlp
    let mut vtt_files: Vec<std::path::PathBuf> = Vec::new();
    let mut info_json: Option<std::path::PathBuf> = None;
    let mut dir = tokio::fs::read_dir(&tmp_path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let path: std::path::PathBuf = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("vtt") => vtt_files.push(path),
            Some("json") => info_json = Some(path),
            _ => {}
        }
    }

    log_info(&format!(
        "youtube yt_dlp_complete vtt_files={}",
        vtt_files.len()
    ));

    if vtt_files.is_empty() {
        return Err(
            "yt-dlp produced no VTT subtitle files — video may have no captions, \
             or yt-dlp needs updating"
                .into(),
        );
    }

    // Parse video metadata from info.json if available
    let video_meta = match info_json {
        Some(ref p) => meta::parse_youtube_info_json(p).await,
        None => None,
    };

    // Build source-specific extra payload once; merged into every chunk's Qdrant point
    let extra = video_meta.as_ref().map(meta::build_youtube_extra_payload);

    let mut count = 0usize;

    /// Maximum VTT file size accepted before reading into memory (50 MiB).
    const MAX_VTT_BYTES: u64 = 50 * 1024 * 1024;

    for vtt_path in &vtt_files {
        let file_meta = tokio::fs::metadata(vtt_path).await?;
        if file_meta.len() > MAX_VTT_BYTES {
            log_warn(&format!(
                "skipping oversized VTT file ({} bytes > {} limit): {}",
                file_meta.len(),
                MAX_VTT_BYTES,
                vtt_path.display()
            ));
            continue;
        }
        let vtt_text = tokio::fs::read_to_string(vtt_path).await?;
        let text = parse_vtt_to_text(&vtt_text);
        if text.trim().is_empty() {
            continue;
        }

        // yt-dlp output template is "%(id)s" so the stem before the first "." is the video ID
        let stem = vtt_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let vid_id = stem.split('.').next().unwrap_or(stem);
        let source_url = format!("https://www.youtube.com/watch?v={vid_id}");
        let title = video_meta
            .as_ref()
            .map(|m| m.title.as_str())
            .unwrap_or(vid_id);

        let mut docs: Vec<PreparedDoc> = Vec::new();

        let transcript_chunks = chunk_text(&text);
        if !transcript_chunks.is_empty() {
            let domain = Url::parse(&source_url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "youtube.com".to_string());
            docs.push(PreparedDoc {
                url: source_url.clone(),
                domain,
                chunks: transcript_chunks,
                source_type: "youtube".to_string(),
                content_type: "text",
                title: Some(title.to_string()),
                extra: extra.clone(),
            });
        }

        // Embed description as a separate document (often contains commands, links, timestamps)
        if let Some(m) = &video_meta
            && !m.description.trim().is_empty()
        {
            let desc_url = format!("{source_url}?section=description");
            let desc_chunks = chunk_text(&m.description);
            if !desc_chunks.is_empty() {
                let domain = Url::parse(&desc_url)
                    .ok()
                    .and_then(|u| u.host_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "youtube.com".to_string());
                docs.push(PreparedDoc {
                    url: desc_url,
                    domain,
                    chunks: desc_chunks,
                    source_type: "youtube".to_string(),
                    content_type: "text",
                    title: Some(format!("{} — description", m.title)),
                    extra: extra.clone(),
                });
            }
        }

        if let Ok(summary) = embed_prepared_docs(cfg, docs, None).await {
            count += summary.chunks_embedded;
        }
    }

    log_done(&format!(
        "command=ingest source=youtube video_id={video_id} chunk_count={count}"
    ));
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_video_id_from_watch_url() {
        let id = extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_from_short_url() {
        let id = extract_video_id("https://youtu.be/dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_passthrough_for_bare_id() {
        // 11-char alphanumeric = bare video ID
        let id = extract_video_id("dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_returns_none_for_garbage() {
        assert_eq!(extract_video_id("not-a-valid-thing"), None);
    }

    #[test]
    fn extract_video_id_from_embed_url() {
        let id = extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_from_shorts_url() {
        let id = extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_from_v_path_url() {
        let id = extract_video_id("https://www.youtube.com/v/dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn extract_video_id_from_mobile_url() {
        let id = extract_video_id("https://m.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn is_playlist_url_detects_list_param() {
        assert!(is_playlist_or_channel_url(
            "https://www.youtube.com/playlist?list=UUZDfnUn74N0WeAPvMqTOrtA"
        ));
    }

    #[test]
    fn is_playlist_url_false_for_single_video() {
        assert!(!is_playlist_or_channel_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
    }

    #[test]
    fn is_playlist_url_detects_at_handle() {
        assert!(is_playlist_or_channel_url(
            "https://www.youtube.com/@SpaceinvaderOne"
        ));
    }

    #[test]
    fn is_playlist_url_detects_channel_path() {
        assert!(is_playlist_or_channel_url(
            "https://www.youtube.com/channel/UCZDfnUn74N0WeAPvMqTOrtA"
        ));
    }

    #[test]
    fn is_playlist_url_detects_c_path() {
        assert!(is_playlist_or_channel_url(
            "https://www.youtube.com/c/SpaceinvaderOne"
        ));
    }

    #[test]
    fn is_playlist_url_false_for_non_youtube() {
        assert!(!is_playlist_or_channel_url(
            "https://vimeo.com/playlist/123"
        ));
    }
}
