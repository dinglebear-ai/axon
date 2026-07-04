//! YouTube acquisition (yt-dlp subprocess -> prepared dump) for
//! `axon source <youtube-target>`.
//!
//! Mirrors [`crate::reddit_acquire`]: classify the target, run a bounded
//! subprocess to fetch video metadata + subtitles, map the raw yt-dlp output
//! into the prepared dump shape the `axon_adapters::youtube` adapter reads
//! (`{"videos":[{video_id,title,channel,...,transcript}]}`), and write it to a
//! **deterministic**, target-derived cache path. The youtube bridge
//! ([`crate::index_youtube_source_with_job`]) then reads that
//! `youtube_dump_path` — this helper does NOT parse the dump; the adapter does.
//!
//! Kept dependency-free of the legacy `axon-ingest` crate: the yt-dlp spawn +
//! timeout logic is ported here, and the raw-JSON → dump mapping is a pure
//! function ([`map::video_dump_json`]) so it is unit-testable with fixtures, no
//! subprocess.
//!
//! Target URLs never appear verbatim in errors — they are URL-redacted so a
//! credentialed target (e.g. a private URL with query params) cannot leak.

mod map;

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axon_adapters::youtube::{YoutubeTarget, parse_youtube_target};
use axon_core::content::redact_url;
use axon_core::http::validate_url;
use sha2::{Digest, Sha256};

use self::map::{parse_vtt_to_text, video_dump_json};

/// Wall-clock cap for the yt-dlp subprocess before it is aborted. Generous for
/// subtitle downloads / playlist enumeration, but bounds a hung process.
const YTDLP_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum single subtitle file size read into memory (50 MiB). A hostile or
/// misconfigured endpoint cannot OOM us via one enormous `.vtt`.
const MAX_SUBTITLE_BYTES: u64 = 50 * 1024 * 1024;

/// Maximum number of videos enumerated from a playlist/channel target.
const MAX_PLAYLIST_VIDEOS: usize = 500;

/// Classify `target`, run yt-dlp to fetch video metadata + English subtitles,
/// map the results into the prepared dump shape, and write it to a
/// **deterministic**, target-derived cache path.
///
/// The returned path is a stable function of the target string (not a random
/// temp name), mirroring the reddit/feed caches. `yt-dlp` must be installed and
/// on PATH (or `AXON_YTDLP`/`YT_DLP`); a missing binary is a clear, actionable
/// error. The target is validated as a YouTube video/playlist/channel *before*
/// any subprocess runs, and the canonical URL passed to yt-dlp is
/// SSRF-validated. Errors are URL-redacted.
pub async fn fetch_youtube_dump(target: &str) -> Result<PathBuf> {
    let parsed = parse_youtube_target(target)
        .map_err(|err| anyhow::anyhow!("invalid youtube target '{target}': {}", err.message))?;

    // Fetch the video(s) into a temp workspace; cleaned up on drop.
    let tmp = tempfile::tempdir().context("failed to create youtube work directory")?;
    let videos = fetch_videos(&parsed, tmp.path()).await?;

    if videos.is_empty() {
        bail!(
            "yt-dlp produced no indexable videos for {} — the video(s) may have no captions, \
             or yt-dlp may need updating",
            redact_url(&parsed.canonical_uri)
        );
    }

    let dump = serde_json::json!({ "videos": videos });
    let bytes = serde_json::to_vec(&dump).context("failed to serialize youtube dump")?;
    let path = youtube_cache_path(target);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create youtube cache directory")?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .with_context(|| format!("failed to write youtube dump for target '{target}'"))?;
    Ok(path)
}

/// Deterministic on-disk path for a youtube target:
/// `<tmp>/axon-youtube/<sha256(target)>.json`.
///
/// Stability (same target -> same path) mirrors the reddit/feed caches. The
/// youtube bridge derives the source id from the *canonical target URI* (not
/// the path), but a stable path still avoids leaking a fresh temp file per run
/// and keeps the dump inspectable/reproducible.
fn youtube_cache_path(target: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(target.trim().as_bytes());
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir()
        .join("axon-youtube")
        .join(format!("{hash}.json"))
}

/// Resolve which yt-dlp binary to invoke. Honors `AXON_YTDLP`/`YT_DLP`
/// overrides, defaulting to `yt-dlp` on PATH.
fn ytdlp_binary() -> String {
    for key in ["AXON_YTDLP", "YT_DLP"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    "yt-dlp".to_string()
}

/// Fetch the target's video(s) into `work_dir` and map each into a dump-shaped
/// JSON value. A single video yields one entry; a playlist/channel enumerates
/// (bounded) video URLs first, then fetches each.
async fn fetch_videos(parsed: &YoutubeTarget, work_dir: &Path) -> Result<Vec<serde_json::Value>> {
    match &parsed.video_id {
        Some(video_id) => {
            let safe_url = format!("https://www.youtube.com/watch?v={video_id}");
            validate_url(&safe_url)
                .map_err(|err| anyhow::anyhow!("youtube target failed SSRF validation: {err}"))?;
            Ok(fetch_single_video(&safe_url, work_dir)
                .await?
                .into_iter()
                .collect())
        }
        None => fetch_playlist(&parsed.canonical_uri, work_dir).await,
    }
}

/// Enumerate a playlist/channel's video URLs (bounded), then fetch each into a
/// dump entry. Videos that yield no transcript are skipped, not fatal.
async fn fetch_playlist(canonical_uri: &str, work_dir: &Path) -> Result<Vec<serde_json::Value>> {
    validate_url(canonical_uri)
        .map_err(|err| anyhow::anyhow!("youtube target failed SSRF validation: {err}"))?;
    let urls = enumerate_playlist_videos(canonical_uri).await?;
    let mut videos = Vec::new();
    for (idx, url) in urls.iter().enumerate() {
        // Each video gets its own subdirectory so filenames don't collide.
        let sub = work_dir.join(format!("v{idx}"));
        tokio::fs::create_dir_all(&sub)
            .await
            .context("failed to create youtube per-video directory")?;
        if let Some(video) = fetch_single_video(url, &sub).await? {
            videos.push(video);
        }
    }
    Ok(videos)
}

/// Run yt-dlp for a single canonical video URL, then read its info json +
/// subtitle transcript and map them into a dump entry. Returns `None` when the
/// video produced no usable transcript.
async fn fetch_single_video(safe_url: &str, work_dir: &Path) -> Result<Option<serde_json::Value>> {
    run_ytdlp_video(safe_url, work_dir).await?;

    let (info_json, vtt_files) = collect_outputs(work_dir).await?;
    let Some(info_path) = info_json else {
        return Ok(None);
    };
    let info_text = tokio::fs::read_to_string(&info_path)
        .await
        .context("failed to read yt-dlp info json")?;
    let info: serde_json::Value =
        serde_json::from_str(&info_text).context("yt-dlp info json was not valid JSON")?;

    let transcript = read_transcript(&vtt_files).await?;
    if transcript.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(video_dump_json(&info, &transcript)))
}

/// Read + concatenate the transcript text from the collected `.vtt` files,
/// skipping any that exceed the size cap.
async fn read_transcript(vtt_files: &[PathBuf]) -> Result<String> {
    let mut parts = Vec::new();
    for vtt_path in vtt_files {
        let meta = tokio::fs::metadata(vtt_path)
            .await
            .context("failed to stat yt-dlp subtitle file")?;
        if meta.len() > MAX_SUBTITLE_BYTES {
            continue;
        }
        let vtt = tokio::fs::read_to_string(vtt_path)
            .await
            .context("failed to read yt-dlp subtitle file")?;
        let text = parse_vtt_to_text(&vtt);
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    Ok(parts.join("\n"))
}

/// Collect the `.info.json` and `.vtt` files yt-dlp wrote into `work_dir`.
async fn collect_outputs(work_dir: &Path) -> Result<(Option<PathBuf>, Vec<PathBuf>)> {
    let mut info_json: Option<PathBuf> = None;
    let mut vtt_files: Vec<PathBuf> = Vec::new();
    let mut dir = tokio::fs::read_dir(work_dir)
        .await
        .context("failed to read youtube work directory")?;
    while let Some(entry) = dir
        .next_entry()
        .await
        .context("failed to enumerate youtube work directory")?
    {
        let path = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("vtt") => vtt_files.push(path),
            Some("json") => info_json = Some(path),
            _ => {}
        }
    }
    Ok((info_json, vtt_files))
}

/// Run yt-dlp to download English auto-subtitles + info json for one video into
/// `work_dir`, writing filenames templated on the video id.
async fn run_ytdlp_video(safe_url: &str, work_dir: &Path) -> Result<()> {
    let output_template = format!("{}/%(id)s", work_dir.to_string_lossy());
    let mut command = tokio::process::Command::new(ytdlp_binary());
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
        &output_template,
        "--",
        safe_url,
    ]);
    let output = run_command_with_timeout(command, "yt-dlp subtitle download").await?;
    if !output.status.success() {
        bail!(
            "yt-dlp failed for {} (exit status {:?})",
            redact_url(safe_url),
            output.status.code()
        );
    }
    Ok(())
}

/// Enumerate the video URLs in a playlist/channel via `yt-dlp --flat-playlist`,
/// canonicalizing each to a `watch?v=` URL and bounding the count.
async fn enumerate_playlist_videos(url: &str) -> Result<Vec<String>> {
    let playlist_end = MAX_PLAYLIST_VIDEOS.to_string();
    let mut command = tokio::process::Command::new(ytdlp_binary());
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
    let output = run_command_with_timeout(command, "yt-dlp --flat-playlist").await?;
    if !output.status.success() {
        bail!(
            "yt-dlp --flat-playlist failed for {} (exit status {:?})",
            redact_url(url),
            output.status.code()
        );
    }
    let rows: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    Ok(map::canonicalize_enumerated_video_rows(rows))
}

/// Spawn a pre-built command with the yt-dlp timeout. Distinguishes a missing
/// binary (actionable install hint) from other spawn/timeout failures.
async fn run_command_with_timeout(
    mut command: tokio::process::Command,
    context: &str,
) -> Result<Output> {
    command.kill_on_drop(true);
    let result = tokio::time::timeout(YTDLP_TIMEOUT, command.output()).await;
    match result {
        Err(_) => bail!("{context} timed out after {}s", YTDLP_TIMEOUT.as_secs()),
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => bail!(
            "yt-dlp is not installed or not on PATH — install it (https://github.com/yt-dlp/yt-dlp) \
             or set AXON_YTDLP to its path"
        ),
        Ok(Err(err)) => bail!("{context}: process failed to start: {err}"),
        Ok(Ok(output)) => Ok(output),
    }
}

#[cfg(test)]
#[path = "youtube_acquire_tests.rs"]
mod tests;
