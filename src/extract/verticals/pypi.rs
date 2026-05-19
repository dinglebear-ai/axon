//! PyPI package vertical extractor.
//!
//! Matches pypi.org/project/{name} or /project/{name}/{version} and fetches
//! metadata from the PyPI JSON API. No authentication required.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "pypi",
    label: "PyPI Package",
    description: "Fetches package metadata from pypi.org/pypi/{name}/json — version, description, classifiers, links, dependencies.",
    url_patterns: &[
        "https://pypi.org/project/{name}",
        "https://pypi.org/project/{name}/{version}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "pypi.org" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /project/{name}  OR  /project/{name}/{version}
    segs.len() >= 2 && segs[0] == "project"
}

/// Aggregated data for building PyPI package markdown.
struct PypiMarkdownData<'a> {
    pkg_name: &'a str,
    version: &'a str,
    summary: &'a str,
    author: &'a str,
    license: &'a str,
    requires_python: &'a str,
    home_page: &'a str,
    keywords: &'a [&'a str],
    classifiers: &'a [&'a str],
    project_urls: &'a serde_json::Map<String, serde_json::Value>,
    requires_dist: &'a [&'a str],
    description: &'a str,
    url: &'a str,
}

/// Build the markdown content from pypi package data.
fn build_pypi_markdown(d: &PypiMarkdownData<'_>) -> String {
    let mut md = format!("# {} {}\n\n", d.pkg_name, d.version);
    if !d.summary.is_empty() {
        md.push_str(d.summary);
        md.push_str("\n\n");
    }
    if !d.author.is_empty() {
        md.push_str(&format!("**Author:** {}\n", d.author));
    }
    if !d.license.is_empty() {
        md.push_str(&format!("**License:** {}\n", d.license));
    }
    if !d.requires_python.is_empty() {
        md.push_str(&format!("**Requires Python:** {}\n", d.requires_python));
    }
    if !d.home_page.is_empty() {
        md.push_str(&format!("**Homepage:** {}\n", d.home_page));
    }
    if !d.keywords.is_empty() {
        md.push_str(&format!("**Keywords:** {}\n", d.keywords.join(", ")));
    }
    // Project URLs
    if !d.project_urls.is_empty() {
        md.push_str("\n**Project Links:**\n");
        for (label, val) in d.project_urls {
            if let Some(u) = val.as_str() {
                md.push_str(&format!("- {label}: {u}\n"));
            }
        }
    }
    // Classifiers (max 20)
    if !d.classifiers.is_empty() {
        md.push_str("\n**Classifiers:**\n");
        for c in d.classifiers.iter().take(20) {
            md.push_str(&format!("- {c}\n"));
        }
    }
    // Dependencies (max 30)
    if !d.requires_dist.is_empty() {
        md.push_str("\n**Dependencies:**\n");
        for dep in d.requires_dist.iter().take(30) {
            md.push_str(&format!("- {dep}\n"));
        }
    }
    md.push_str(&format!("\n**PyPI:** {}\n", d.url));
    // Long description / README (truncated to 50_000 chars)
    if !d.description.is_empty() {
        md.push_str("\n## Description\n\n");
        let truncated: String = d.description.chars().take(50_000).collect();
        md.push_str(&truncated);
        md.push('\n');
    }
    md
}

/// Fetch PyPI JSON data for `name` (optionally pinned to `version`).
async fn fetch_pypi_data(
    name: &str,
    version_seg: Option<&str>,
    ctx: &VerticalContext,
    url: &str,
) -> Result<serde_json::Value, VerticalError> {
    let api_url = match version_seg {
        Some(v) => format!("https://pypi.org/pypi/{name}/{v}/json"),
        None => format!("https://pypi.org/pypi/{name}/json"),
    };
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
    match status {
        404 => {
            return Err(VerticalError::VerticalTargetNotFound {
                vertical: INFO.name,
                url: url.to_string(),
            });
        }
        429 => {
            return Err(VerticalError::VerticalRateLimited {
                vertical: INFO.name,
                retry_after: None,
            });
        }
        200 => {}
        _ => {
            return Err(VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            });
        }
    }
    resp.json()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        })
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "project" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    let name = segs[1];
    let version_seg = segs.get(2).copied();
    let data = fetch_pypi_data(name, version_seg, ctx, url).await?;

    let info = &data["info"];
    let pkg_name = info["name"].as_str().unwrap_or(name);
    let version = info["version"].as_str().unwrap_or("unknown");
    let summary = info["summary"].as_str().unwrap_or("");
    let author = info["author"].as_str().unwrap_or("");
    let license = info["license"].as_str().unwrap_or("");
    let home_page = info["home_page"].as_str().unwrap_or("");
    let requires_python = info["requires_python"].as_str().unwrap_or("");
    let description = info["description"].as_str().unwrap_or("");

    // keywords is a comma-separated string
    let keywords_str = info["keywords"].as_str().unwrap_or("");
    let keywords: Vec<&str> = keywords_str
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let classifiers: Vec<&str> = info["classifiers"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let requires_dist: Vec<&str> = info["requires_dist"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let empty_map = serde_json::Map::new();
    let project_urls = info["project_urls"].as_object().unwrap_or(&empty_map);

    // Build follow_crawl_urls from all project URL values
    let follow_crawl_urls: Vec<String> = project_urls
        .values()
        .filter_map(|v| v.as_str())
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .map(str::to_string)
        .collect();

    let title = Some(format!("{pkg_name} {version}"));
    let md = build_pypi_markdown(&PypiMarkdownData {
        pkg_name,
        version,
        summary,
        author,
        license,
        requires_python,
        home_page,
        keywords: &keywords,
        classifiers: &classifiers,
        project_urls,
        requires_dist: &requires_dist,
        description,
        url,
    });

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls,
    })
}
