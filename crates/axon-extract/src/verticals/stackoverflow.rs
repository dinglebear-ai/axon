//! Stack Overflow vertical extractor via Stack Exchange API v2.3.
//!
//! Matches stackoverflow.com/questions/{id}[/{slug}].
//! Fetches question + top answers in parallel.
//! Rate limit: 300 requests/day unauthenticated (no key required).
//!
//! auto_dispatch: true

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "stackoverflow",
    label: "Stack Overflow Question",
    description: "Fetches Stack Overflow question + top answers from the Stack Exchange API.",
    url_patterns: &[
        "https://stackoverflow.com/questions/{id}",
        "https://stackoverflow.com/questions/{id}/{slug}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "stackoverflow.com" {
        return false;
    }
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // /questions/{numeric_id} or /questions/{numeric_id}/{slug}
    segs.len() >= 2 && segs[0] == "questions" && segs[1].parse::<u64>().is_ok()
}

fn extract_question_id(url: &str) -> Option<u64> {
    let parsed = url::Url::parse(url).ok()?;
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segs.len() >= 2 && segs[0] == "questions" {
        return segs[1].parse::<u64>().ok();
    }
    None
}

/// Strip HTML tags for plain text display.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let qid = extract_question_id(url).ok_or(VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let q_url = format!(
        "https://api.stackexchange.com/2.3/questions/{qid}?site=stackoverflow&filter=withbody"
    );
    let a_url = format!(
        "https://api.stackexchange.com/2.3/questions/{qid}/answers?site=stackoverflow&filter=withbody&order=desc&sort=votes"
    );

    let (q_resp, a_resp) = tokio::join!(
        client.get(&q_url).header("User-Agent", ctx.api_ua()).send(),
        client.get(&a_url).header("User-Agent", ctx.api_ua()).send(),
    );

    let q_resp = q_resp.map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;
    let q_status = q_resp.status().as_u16();
    if q_status == 404 {
        return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }
    if q_status == 429 {
        return Err(VerticalError::VerticalRateLimited {
            vertical: INFO.name,
            retry_after: None,
        });
    }
    if q_status != 200 {
        return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: q_status,
        });
    }

    let q_data: serde_json::Value =
        q_resp
            .json()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status: q_status,
            })?;

    // Parse answers (non-fatal if API call failed)
    let a_data: Option<serde_json::Value> = if let Ok(ar) = a_resp {
        ar.json().await.ok()
    } else {
        None
    };

    let question = q_data["items"].as_array().and_then(|a| a.first()).ok_or(
        VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        },
    )?;

    let answers: &[serde_json::Value] = a_data
        .as_ref()
        .and_then(|d| d["items"].as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    build_scraped_doc(url, question, answers)
}

fn build_extra(question: &serde_json::Value, date_str: &str) -> serde_json::Value {
    let question_id = question["question_id"].as_u64().unwrap_or(0);
    let score = question["score"].as_i64().unwrap_or(0);
    let view_count = question["view_count"].as_u64().unwrap_or(0);
    let is_answered = question["is_answered"].as_bool().unwrap_or(false);
    let answer_count = question["answer_count"].as_u64().unwrap_or(0);
    let author = question["owner"]["display_name"].as_str().unwrap_or("");
    let tags: Vec<&str> = question["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let mut obj = serde_json::json!({
        "so_question_id": question_id,
        "so_score": score,
        "so_view_count": view_count,
        "so_is_answered": if is_answered { "true" } else { "false" },
        "so_answer_count": answer_count,
    });
    if !tags.is_empty() {
        obj["so_tags"] = serde_json::json!(tags);
    }
    if !author.is_empty() {
        obj["so_author"] = serde_json::Value::String(author.to_string());
    }
    if !date_str.is_empty() {
        obj["so_created_at"] = serde_json::Value::String(date_str.to_string());
    }
    obj
}

fn build_scraped_doc(
    url: &str,
    question: &serde_json::Value,
    answers: &[serde_json::Value],
) -> Result<ScrapedDoc, VerticalError> {
    let title = question["title"].as_str().unwrap_or("Untitled").to_string();
    let body_html = question["body"].as_str().unwrap_or("");
    let body_text = strip_html_tags(body_html);
    let tags: Vec<&str> = question["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let score = question["score"].as_i64().unwrap_or(0);
    let view_count = question["view_count"].as_u64().unwrap_or(0);
    let is_answered = question["is_answered"].as_bool().unwrap_or(false);
    let creation_date = question["creation_date"].as_u64().unwrap_or(0);
    let q_author = question["owner"]["display_name"]
        .as_str()
        .unwrap_or("unknown");
    let answer_count = question["answer_count"].as_u64().unwrap_or(0);
    let date_str = if creation_date > 0 {
        format_unix_ts(creation_date)
    } else {
        String::new()
    };

    let mut md = format!("# {title}\n\n");
    let tags_str = tags.join(", ");
    let answered_note = if is_answered { " ✓ Answered" } else { "" };
    md.push_str(&format!(
        "**Tags:** {tags_str} | **Score:** {score} | **Views:** {view_count} | **Asked:** {date_str}{answered_note}\n"
    ));
    md.push_str("\n## Question\n\n");
    md.push_str(&body_text);
    md.push('\n');

    if !answers.is_empty() {
        md.push_str(&format!("\n## Answers ({answer_count} total)\n\n"));
        for answer in answers.iter().take(5) {
            let a_score = answer["score"].as_i64().unwrap_or(0);
            let a_accepted = answer["is_accepted"].as_bool().unwrap_or(false);
            let a_author = answer["owner"]["display_name"]
                .as_str()
                .unwrap_or("unknown");
            let a_body_html = answer["body"].as_str().unwrap_or("");
            let a_body = strip_html_tags(a_body_html);
            let a_excerpt: String = a_body.chars().take(2000).collect();
            let ellipsis = if a_body.len() > 2000 { "…" } else { "" };
            let accepted_tag = if a_accepted { " [Accepted]" } else { "" };
            md.push_str(&format!(
                "### Answer by {a_author} (score: {a_score}){accepted_tag}\n\n{a_excerpt}{ellipsis}\n\n"
            ));
        }
    }

    md.push_str(&format!("**Stack Overflow:** {url}\n"));

    let structured = serde_json::json!({
        "question_id": question["question_id"],
        "title": title,
        "score": score,
        "view_count": view_count,
        "is_answered": is_answered,
        "tags": tags,
        "answer_count": answer_count,
        "author": q_author,
    });

    let extra = build_extra(question, &date_str);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(title),
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(structured),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

fn format_unix_ts(ts: u64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
#[path = "stackoverflow_tests.rs"]
mod tests;
