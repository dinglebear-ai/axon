mod meta;
mod vtt;

pub use vtt::parse_vtt_to_text;

use crate::core::config::Config;
use crate::core::content::url_to_domain;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::{
    MAX_INGEST_FILE_BYTES, SUBPROCESS_TIMEOUT, run_command_with_timeout,
};
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};
use spider::url::Url;
use std::error::Error;

const PHASE_DOWNLOADING: &str = "downloading_transcript";
const PHASE_PARSING: &str = "parsing_transcript";
const PHASE_EMBEDDING: &str = "embedding_transcript";

const MAX_PLAYLIST_VIDEOS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeTargetKind {
    SingleVideo,
    PlaylistOrChannel,
}

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

fn normalize_youtube_target(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed.starts_with('@') {
        format!("https://www.youtube.com/{trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub fn classify_youtube_target(target: &str) -> Result<YoutubeTargetKind, &'static str> {
    let normalized = normalize_youtube_target(target);
    if is_playlist_or_channel_url(&normalized) {
        return Ok(YoutubeTargetKind::PlaylistOrChannel);
    }
    if extract_video_id(&normalized).is_some() {
        return Ok(YoutubeTargetKind::SingleVideo);
    }
    Err("target does not appear to be a YouTube video, playlist, or channel")
}

pub fn canonicalize_enumerated_video_rows(rows: Vec<String>) -> Vec<String> {
    rows.into_iter()
        .filter_map(|row| {
            let trimmed = row.trim();
            if trimmed.is_empty() {
                log_warn("youtube playlist enumeration skipped empty row");
                return None;
            }
            match extract_video_id(trimmed) {
                Some(id) => Some(format!("https://www.youtube.com/watch?v={id}")),
                None => {
                    log_warn(&format!(
                        "youtube playlist enumeration skipped invalid row={trimmed}"
                    ));
                    None
                }
            }
        })
        .collect()
}

pub async fn enumerate_playlist_videos(url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    validate_url(url)?;

    let playlist_end = MAX_PLAYLIST_VIDEOS.to_string();
    let mut command = tokio::process::Command::new("yt-dlp");
    command.args([
        "--flat-playlist",
        "--print",
        "%(url)s",
        "--playlist-end",
        &playlist_end,
        "--no-exec",
        "--",
        url,
    ]);

    let output =
        run_command_with_timeout(command, SUBPROCESS_TIMEOUT, "yt-dlp --flat-playlist").await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp --flat-playlist exited non-zero: {stderr}").into());
    }

    let rows: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .collect();

    Ok(canonicalize_enumerated_video_rows(rows))
}

async fn run_ytdlp(safe_url: &str, tmp_path: &str) -> Result<(), Box<dyn Error>> {
    let mut command = tokio::process::Command::new("yt-dlp");
    command.args([
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
    ]);

    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, "yt-dlp subtitle download")
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;

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
pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
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

    reporter.report_phase(PHASE_DOWNLOADING).await;

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

    reporter.report_phase(PHASE_PARSING).await;

    // Parse video metadata from info.json if available
    let video_meta = match info_json {
        Some(ref p) => meta::parse_youtube_info_json(p).await,
        None => None,
    };

    // Build source-specific extra payload once; merged into every chunk's Qdrant point
    let extra = video_meta.as_ref().map(meta::build_youtube_extra_payload);

    let mut count = 0usize;

    for vtt_path in &vtt_files {
        let file_meta = tokio::fs::metadata(vtt_path).await?;
        if file_meta.len() > MAX_INGEST_FILE_BYTES {
            log_warn(&format!(
                "skipping oversized VTT file ({} bytes > {} limit): {}",
                file_meta.len(),
                MAX_INGEST_FILE_BYTES,
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
            docs.push(PreparedDoc {
                url: source_url.clone(),
                domain: url_to_domain(&source_url),
                chunks: transcript_chunks,
                source_type: "youtube".to_string(),
                content_type: "text",
                title: Some(title.to_string()),
                extra: extra.clone(),
                extractor_name: None,
            });
        }

        // Embed description as a separate document (often contains commands, links, timestamps)
        if let Some(m) = &video_meta
            && !m.description.trim().is_empty()
        {
            let desc_url = format!("{source_url}?section=description");
            let desc_chunks = chunk_text(&m.description);
            if !desc_chunks.is_empty() {
                docs.push(PreparedDoc {
                    url: desc_url.clone(),
                    domain: url_to_domain(&desc_url),
                    chunks: desc_chunks,
                    source_type: "youtube".to_string(),
                    content_type: "text",
                    title: Some(format!("{} — description", m.title)),
                    extra: extra.clone(),
                    extractor_name: None,
                });
            }
        }

        reporter.report_phase(PHASE_EMBEDDING).await;
        match embed_prepared_docs(cfg, docs, None).await {
            Ok(summary) => count += summary.chunks_embedded,
            Err(e) => log_warn(&format!(
                "command=ingest source=youtube embed_failed video={vid_id} err={e}"
            )),
        }
    }

    log_done(&format!(
        "command=ingest source=youtube video_id={video_id} chunk_count={count}"
    ));
    Ok(count)
}

pub async fn ingest_youtube_target(
    cfg: &Config,
    target: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    let normalized = normalize_youtube_target(target);
    match classify_youtube_target(&normalized)? {
        YoutubeTargetKind::SingleVideo => ingest_youtube(cfg, &normalized, reporter).await,
        YoutubeTargetKind::PlaylistOrChannel => {
            ingest_youtube_playlist(cfg, &normalized, reporter).await
        }
    }
}

pub async fn ingest_youtube_playlist(
    cfg: &Config,
    target: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info(&format!(
        "command=ingest source=youtube playlist target={target}"
    ));
    reporter.report_phase("enumerating_videos").await;
    let videos = enumerate_playlist_videos(target).await?;
    if videos.is_empty() {
        return Err("yt-dlp produced no valid YouTube video rows".into());
    }

    let videos_total = videos.len();
    let mut chunks_embedded = 0usize;
    reporter
        .report(serde_json::json!({
            "phase": "embedding_playlist",
            "videos_done": 0,
            "videos_total": videos_total,
            "chunks_embedded": 0,
        }))
        .await;

    for (idx, video_url) in videos.iter().enumerate() {
        match ingest_youtube(cfg, video_url, reporter).await {
            Ok(chunks) => chunks_embedded += chunks,
            Err(e) => log_warn(&format!(
                "command=ingest source=youtube playlist video_failed url={video_url} err={e}"
            )),
        }
        reporter
            .report(serde_json::json!({
                "phase": "embedding_playlist",
                "videos_done": idx + 1,
                "videos_total": videos_total,
                "chunks_embedded": chunks_embedded,
            }))
            .await;
    }

    log_done(&format!(
        "command=ingest source=youtube playlist videos_total={videos_total} chunk_count={chunks_embedded}"
    ));
    Ok(chunks_embedded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_video_ids_from_supported_target_forms() {
        for input in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
        ] {
            assert_eq!(extract_video_id(input), Some("dQw4w9WgXcQ".to_string()));
        }
    }

    #[test]
    fn detects_playlist_and_channel_urls() {
        for input in [
            "https://www.youtube.com/playlist?list=UUZDfnUn74N0WeAPvMqTOrtA",
            "https://www.youtube.com/@SpaceinvaderOne",
            "https://www.youtube.com/channel/UCZDfnUn74N0WeAPvMqTOrtA",
            "https://www.youtube.com/c/SpaceinvaderOne",
        ] {
            assert!(is_playlist_or_channel_url(input));
        }
        assert!(!is_playlist_or_channel_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
    }

    #[test]
    fn canonicalizes_enumerated_playlist_rows_to_watch_urls() {
        let rows = vec![
            "dQw4w9WgXcQ".to_string(),
            " https://youtu.be/abcDEF123_4 ".to_string(),
            "https://www.youtube.com/shorts/ZYxwvUTsr-1".to_string(),
            "".to_string(),
            "not a video".to_string(),
        ];

        assert_eq!(
            canonicalize_enumerated_video_rows(rows),
            vec![
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                "https://www.youtube.com/watch?v=abcDEF123_4",
                "https://www.youtube.com/watch?v=ZYxwvUTsr-1",
            ]
        );
    }

    #[test]
    fn classifies_youtube_targets_for_source_dispatch() {
        for (target, kind) in [
            ("dQw4w9WgXcQ", YoutubeTargetKind::SingleVideo),
            (
                "https://www.youtube.com/@SpaceinvaderOne",
                YoutubeTargetKind::PlaylistOrChannel,
            ),
            ("@SpaceinvaderOne", YoutubeTargetKind::PlaylistOrChannel),
        ] {
            assert_eq!(classify_youtube_target(target).unwrap(), kind);
        }
        assert!(classify_youtube_target("https://example.com/nope").is_err());
    }
}
