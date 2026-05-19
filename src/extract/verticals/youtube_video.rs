//! YouTube video vertical extractor.
//!
//! Fetches `/watch?v={id}` HTML and extracts the `ytInitialPlayerResponse` JSON blob
//! for rich metadata. Falls back to OG tags on consent/age-gate pages.
//!
//! auto_dispatch: true — YouTube is a unique host with parseable metadata.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "youtube_video",
    label: "YouTube Video",
    description: "Extracts YouTube video metadata from ytInitialPlayerResponse. Falls back to OG tags.",
    url_patterns: &["https://youtube.com/watch?v={id}", "https://youtu.be/{id}"],
    auto_dispatch: true,
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
        let segs: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segs.len() >= 2 && matches!(segs[0], "shorts" | "live" | "embed") {
            return true;
        }
    }
    false
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(url)
        .header("User-Agent", ctx.ua())
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    if status == 404 {
        return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }
    if status != 200 {
        return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        });
    }

    let body = resp
        .text()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        })?;

    let video_id = extract_video_id(url).unwrap_or_else(|| "unknown".to_string());

    if let Some(player) = extract_player_response(&body) {
        build_from_player(url, &video_id, player)
    } else {
        build_from_og(&body, url, &video_id)
    }
}

/// Extract the `ytInitialPlayerResponse` JSON object from the page HTML.
fn extract_player_response(html: &str) -> Option<serde_json::Value> {
    static PLAYER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PLAYER_RE.get_or_init(|| {
        regex::Regex::new(r"var\s+ytInitialPlayerResponse\s*=\s*(\{.+?\})\s*;")
            .expect("static regex")
    });
    // Try regex first (fast path)
    if let Some(cap) = re.captures(html) {
        let candidate = &cap[1];
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
            return Some(v);
        }
    }
    // Fallback: brace-count from assignment position
    brace_count_player_response(html)
}

/// Brace-counting fallback: finds `ytInitialPlayerResponse = {` and counts
/// braces to locate the end of the JSON object.
fn brace_count_player_response(html: &str) -> Option<serde_json::Value> {
    let marker = "ytInitialPlayerResponse = ";
    let start_pos = html.find(marker)?;
    let after = &html[start_pos + marker.len()..];
    let obj_start = after.find('{')?;
    let slice = &after[obj_start..];
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in slice.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_str => escape = true,
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    let json_slice = &slice[..=i];
                    return serde_json::from_str(json_slice).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn format_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn build_from_player(
    url: &str,
    video_id: &str,
    player: serde_json::Value,
) -> Result<ScrapedDoc, VerticalError> {
    let vd = &player["videoDetails"];
    let mf = &player["microformat"]["playerMicroformatRenderer"];

    let title = vd["title"].as_str().unwrap_or(video_id).to_string();
    let author = vd["author"].as_str().unwrap_or("").to_string();
    let view_count = vd["viewCount"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let duration_secs = vd["lengthSeconds"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let duration = format_duration(duration_secs);
    let description = vd["shortDescription"].as_str().unwrap_or("").to_string();
    let keywords: Vec<&str> = vd["keywords"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let upload_date = mf["uploadDate"].as_str().unwrap_or("").to_string();
    let category = mf["category"].as_str().unwrap_or("").to_string();
    let view_fmt = format_view_count(view_count);

    let caption_tracks = extract_caption_tracks(&player);

    let mut md = format!("# {title}\n\n");
    md.push_str(&format!(
        "**Channel:** {author} | **Views:** {view_fmt} | **Published:** {upload_date} | **Duration:** {duration}\n"
    ));
    if !category.is_empty() {
        md.push_str(&format!("\n**Category:** {category}\n"));
    }
    if !description.is_empty() {
        md.push_str("\n## Description\n\n");
        let excerpt: String = description.chars().take(2000).collect();
        md.push_str(&excerpt);
        md.push('\n');
    }
    if !keywords.is_empty() {
        md.push_str(&format!("\n**Keywords:** {}\n", keywords.join(", ")));
    }
    if !caption_tracks.is_empty() {
        md.push_str(&format!("\n**Captions:** {}\n", caption_tracks.join(", ")));
    }
    md.push_str(&format!("\n**YouTube:** {url}\n"));

    let structured = serde_json::json!({
        "title": title,
        "author": author,
        "view_count": view_count,
        "duration_seconds": duration_secs,
        "upload_date": upload_date,
        "category": category,
        "keywords": keywords,
        "data_source": "ytInitialPlayerResponse",
    });

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(title),
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(structured),
        follow_crawl_urls: vec![],
    })
}

fn extract_caption_tracks(player: &serde_json::Value) -> Vec<String> {
    player["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"]
        .as_array()
        .map(|tracks| {
            tracks
                .iter()
                .filter_map(|t| {
                    let lang = t["languageCode"].as_str()?;
                    let name = t["name"]["simpleText"].as_str().unwrap_or(lang);
                    Some(format!("{name} ({lang})"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn format_view_count(count: u64) -> String {
    if count >= 1_000_000_000 {
        format!("{:.1}B", count as f64 / 1_000_000_000.0)
    } else if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// OG tag fallback for consent/age-gate pages.
fn build_from_og(html: &str, url: &str, video_id: &str) -> Result<ScrapedDoc, VerticalError> {
    let og_title = extract_og(html, "og:title").unwrap_or_else(|| format!("YouTube {video_id}"));
    let og_desc = extract_og(html, "og:description").unwrap_or_default();

    let mut md = format!("# {og_title}\n\n");
    if !og_desc.is_empty() {
        md.push_str(&og_desc);
        md.push('\n');
    }
    md.push_str(&format!("\n**YouTube:** {url}\n"));

    let structured = serde_json::json!({
        "title": og_title,
        "description": og_desc,
        "data_source": "og_fallback",
    });

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(og_title),
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(structured),
        follow_crawl_urls: vec![],
    })
}

fn extract_og(html: &str, property: &str) -> Option<String> {
    let needle = format!(r#"property="{property}""#);
    let pos = html.find(&needle)?;
    let after = &html[pos..];
    let content_pos = after.find(r#"content=""#)?;
    let value_start = content_pos + r#"content=""#.len();
    let value_slice = &after[value_start..];
    let end = value_slice.find('"')?;
    Some(value_slice[..end].to_string())
}

fn extract_video_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "youtu.be" {
        let id = parsed.path().trim_matches('/').to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    if host == "youtube.com" || host == "www.youtube.com" || host == "m.youtube.com" {
        for (k, v) in parsed.query_pairs() {
            if k == "v" {
                return Some(v.into_owned());
            }
        }
        let segs: Vec<&str> = parsed
            .path()
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segs.len() >= 2 && matches!(segs[0], "shorts" | "live" | "embed") {
            return Some(segs[1].to_string());
        }
    }
    None
}

#[cfg(test)]
#[path = "youtube_video_tests.rs"]
mod tests;
