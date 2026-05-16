//! YouTube video vertical extractor (stub).
//!
//! Matches youtube.com/watch?v={id} and youtu.be/{id} URLs.
//! auto_dispatch is false because yt-dlp invocation is too slow for
//! automatic dispatch during crawl. Use `axon ingest` for full transcript
//! extraction via the yt-dlp pipeline.

use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "youtube_video",
    label: "YouTube Video",
    description: "Matches YouTube video URLs. Use `axon ingest` for full transcript extraction via yt-dlp.",
    url_patterns: &[
        "https://youtube.com/watch?v={id}",
        "https://youtu.be/{id}",
    ],
    auto_dispatch: false,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "youtu.be" {
        return !parsed.path().trim_matches('/').is_empty();
    }
    if host == "youtube.com" || host == "www.youtube.com" || host == "m.youtube.com" {
        let path = parsed.path();
        if path == "/watch" {
            return parsed.query_pairs().any(|(k, _)| k == "v");
        }
        // /shorts/{id} or /live/{id}
        let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 2 && matches!(segs[0], "shorts" | "live" | "embed") {
            return true;
        }
    }
    false
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    // Extract video ID for the stub response
    let video_id = extract_video_id_simple(url).unwrap_or_else(|| "unknown".to_string());
    let title = Some(format!("YouTube video {video_id}"));
    let md = format!(
        "# YouTube Video: {video_id}\n\n\
         This vertical matched the YouTube video URL but does not fetch transcript data.\n\
         To extract full transcript and metadata, use:\n\n\
         ```\n\
         axon ingest {url}\n\
         ```\n\n\
         The `ingest` command uses yt-dlp to download VTT captions and embed the full transcript.\n\n\
         **YouTube:** {url}\n"
    );

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 0,
        structured: None,
    })
}

fn extract_video_id_simple(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "youtu.be" {
        let id = parsed.path().trim_matches('/').to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    if host == "youtube.com" || host == "www.youtube.com" {
        for (k, v) in parsed.query_pairs() {
            if k == "v" {
                return Some(v.into_owned());
            }
        }
        let segs: Vec<&str> = parsed.path().trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 2 && matches!(segs[0], "shorts" | "live" | "embed") {
            return Some(segs[1].to_string());
        }
    }
    None
}
