//! arXiv vertical extractor via Atom XML API.
//!
//! Matches arxiv.org/abs/{id} and arxiv.org/pdf/{id}.
//! Fetches metadata from https://export.arxiv.org/api/query?id_list={id}.
//! Uses regex to parse the Atom XML — no new crate deps.
//!
//! auto_dispatch: true

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "arxiv",
    label: "arXiv Paper",
    description: "Fetches arXiv paper metadata from the Atom XML API — title, authors, abstract, categories.",
    url_patterns: &["https://arxiv.org/abs/{id}", "https://arxiv.org/pdf/{id}"],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "arxiv.org" {
        return false;
    }
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    segs.len() >= 2 && matches!(segs[0], "abs" | "pdf")
}

fn extract_arxiv_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segs.len() >= 2 && matches!(segs[0], "abs" | "pdf") {
        // Join remaining segments to handle old-style IDs like cs.LG/0000000
        let raw = segs[1..].join("/");
        let id = raw.trim_end_matches(".pdf");
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

/// Extract all matches of a simple `<tag>content</tag>` pattern from a slice.
fn extract_all_tags(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut results = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find(&open) {
        let after = &remaining[start + open.len()..];
        if let Some(end) = after.find(&close) {
            results.push(after[..end].trim().to_string());
            remaining = &after[end + close.len()..];
        } else {
            break;
        }
    }
    results
}

/// Extract the first match of `<tag>content</tag>` from a slice.
fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    extract_all_tags(xml, tag).into_iter().next()
}

/// Extract `<name>` values from `<author><name>…</name></author>` blocks.
fn extract_authors(xml: &str) -> Vec<String> {
    let mut authors = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find("<author>") {
        let after = &remaining[start + "<author>".len()..];
        if let Some(end) = after.find("</author>") {
            let author_block = &after[..end];
            if let Some(name) = extract_tag(author_block, "name") {
                authors.push(name);
            }
            remaining = &after[end + "</author>".len()..];
        } else {
            break;
        }
    }
    authors
}

/// Extract `term` attributes from `<category term="cs.LG" .../>` elements.
fn extract_categories(xml: &str) -> Vec<String> {
    static CAT_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = CAT_RE.get_or_init(|| {
        regex::Regex::new(r#"<category\s[^>]*term="([^"]+)""#).expect("static regex")
    });
    re.captures_iter(xml)
        .map(|cap| cap[1].to_string())
        .collect()
}

/// Extract the PDF URL from `<link title="pdf" href="..." />`.
fn extract_pdf_url(xml: &str) -> Option<String> {
    static PDF_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PDF_RE.get_or_init(|| {
        regex::Regex::new(r#"<link[^>]+title="pdf"[^>]+href="([^"]+)""#).expect("static regex")
    });
    re.captures(xml).map(|cap| cap[1].to_string())
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let arxiv_id = extract_arxiv_id(url).ok_or(VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;

    let api_url = format!("https://export.arxiv.org/api/query?id_list={arxiv_id}");
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    if status != 200 {
        return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        });
    }

    let xml = resp
        .text()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        })?;

    // Scope extraction to the <entry> block to skip the feed-level <title>/<summary>
    let entry_xml = extract_entry_block(&xml).ok_or(VerticalError::VerticalTargetNotFound {
        vertical: INFO.name,
        url: url.to_string(),
    })?;

    build_scraped_doc(url, &arxiv_id, entry_xml, &xml)
}

/// Extract the `<entry>…</entry>` block from Atom XML.
fn extract_entry_block(xml: &str) -> Option<&str> {
    let start = xml.find("<entry>")?;
    let after = &xml[start + "<entry>".len()..];
    let end = after.find("</entry>")?;
    Some(&after[..end])
}

fn build_extra(
    arxiv_id: &str,
    authors: &[String],
    categories: &[String],
    published: &str,
    pdf_url: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({ "arxiv_id": arxiv_id });
    if !authors.is_empty() {
        obj["arxiv_authors"] = serde_json::json!(authors);
    }
    if !categories.is_empty() {
        obj["arxiv_categories"] = serde_json::json!(categories);
    }
    if !published.is_empty() {
        obj["arxiv_published"] = serde_json::Value::String(published.to_string());
    }
    if !pdf_url.is_empty() {
        obj["arxiv_pdf_url"] = serde_json::Value::String(pdf_url.to_string());
    }
    obj
}

fn build_scraped_doc(
    url: &str,
    arxiv_id: &str,
    entry_xml: &str,
    full_xml: &str,
) -> Result<ScrapedDoc, VerticalError> {
    let title = extract_tag(entry_xml, "title").unwrap_or_else(|| format!("arXiv:{arxiv_id}"));
    let summary = extract_tag(entry_xml, "summary").unwrap_or_default();
    let published = extract_tag(entry_xml, "published").unwrap_or_default();
    let authors = extract_authors(entry_xml);
    let categories = extract_categories(entry_xml);
    let canonical_id =
        extract_tag(entry_xml, "id").unwrap_or_else(|| format!("https://arxiv.org/abs/{arxiv_id}"));
    let pdf_url =
        extract_pdf_url(full_xml).unwrap_or_else(|| format!("https://arxiv.org/pdf/{arxiv_id}"));

    let authors_str = authors.join(", ");
    let cats_str = categories.join(", ");

    let mut md = format!("# {title}\n\n");
    md.push_str(&format!("**Authors:** {authors_str}\n"));
    if !cats_str.is_empty() {
        md.push_str(&format!("**Categories:** {cats_str}\n"));
    }
    if !published.is_empty() {
        md.push_str(&format!("**Published:** {published}\n"));
    }
    md.push_str("\n## Abstract\n\n");
    md.push_str(summary.trim());
    md.push('\n');
    md.push_str(&format!("\n**arXiv:** {canonical_id}\n"));
    md.push_str(&format!("**PDF:** {pdf_url}\n"));

    let structured = serde_json::json!({
        "arxiv_id": arxiv_id,
        "title": title,
        "authors": authors,
        "categories": categories,
        "published": published,
        "abstract": summary,
        "canonical_url": canonical_id,
        "pdf_url": pdf_url,
    });

    let extra = build_extra(arxiv_id, &authors, &categories, &published, &pdf_url);

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

#[cfg(test)]
#[path = "arxiv_tests.rs"]
mod tests;
