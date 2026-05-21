//! npm package vertical extractor.
//!
//! Matches npmjs.com/package/{name} (including scoped @scope/name packages)
//! and fetches metadata from the npm registry API. No authentication required.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "npm",
    label: "npm Package",
    description: "Fetches package metadata from registry.npmjs.org — version, description, author, license, readme, keywords, repository.",
    url_patterns: &[
        "https://npmjs.com/package/{name}",
        "https://npmjs.com/package/@{scope}/{name}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "npmjs.com" && host != "www.npmjs.com" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /package/{name}  OR  /package/@scope/name
    segs.len() >= 2 && segs[0] == "package"
}

/// Extract author name from the JSON value (string or object).
fn extract_author(data: &serde_json::Value) -> String {
    if let Some(s) = data["author"].as_str() {
        s.to_string()
    } else if let Some(obj) = data["author"].as_object() {
        obj.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    }
}

/// Clean a repository URL: strip git+ prefix, .git suffix, ssh/git schemes.
fn clean_repo_url(raw: &str) -> Option<String> {
    let s = raw.trim();
    // Strip git+ prefix
    let s = s.strip_prefix("git+").unwrap_or(s);
    // Only keep http/https
    if !s.starts_with("https://") && !s.starts_with("http://") {
        return None;
    }
    // Strip .git suffix
    let s = s.strip_suffix(".git").unwrap_or(s);
    Some(s.to_string())
}

/// Extract repository URL string from npm JSON (string or object).
fn extract_repo_url(data: &serde_json::Value) -> Option<String> {
    if let Some(s) = data["repository"].as_str() {
        return clean_repo_url(s);
    }
    if let Some(obj) = data["repository"].as_object() {
        let url = obj.get("url").and_then(|v| v.as_str())?;
        return clean_repo_url(url);
    }
    None
}

/// Aggregated data for building npm package markdown.
struct NpmMarkdownData<'a> {
    name: &'a str,
    latest_version: &'a str,
    description: &'a str,
    author: &'a str,
    license: &'a str,
    homepage: &'a str,
    repo_url: Option<&'a str>,
    keywords: &'a [&'a str],
    engines: &'a serde_json::Value,
    readme: &'a str,
    url: &'a str,
}

/// Build the markdown output for an npm package.
fn build_npm_markdown(d: &NpmMarkdownData<'_>) -> String {
    let mut md = format!("# {}@{}\n\n", d.name, d.latest_version);
    if !d.description.is_empty() {
        md.push_str(d.description);
        md.push_str("\n\n");
    }
    if !d.author.is_empty() {
        md.push_str(&format!("**Author:** {}\n", d.author));
    }
    if !d.license.is_empty() {
        md.push_str(&format!("**License:** {}\n", d.license));
    }
    if !d.homepage.is_empty() {
        md.push_str(&format!("**Homepage:** {}\n", d.homepage));
    }
    if let Some(repo) = d.repo_url {
        md.push_str(&format!("**Repository:** {repo}\n"));
    }
    if !d.keywords.is_empty() {
        md.push_str(&format!("**Keywords:** {}\n", d.keywords.join(", ")));
    }
    if let Some(obj) = d.engines.as_object() {
        let eng: Vec<String> = obj
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}: {s}")))
            .collect();
        if !eng.is_empty() {
            md.push_str(&format!("**Engines:** {}\n", eng.join(", ")));
        }
    }
    md.push_str(&format!("\n**npm:** {}\n", d.url));
    if !d.readme.is_empty() {
        md.push_str("\n## README\n\n");
        md.push_str(d.readme);
        md.push('\n');
    }
    md
}

fn build_extra(
    name: &str,
    version: &str,
    license: &str,
    author: &str,
    keywords: &[&str],
    homepage: &str,
    repo_url: Option<&str>,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "pkg_registry": "npm",
        "pkg_name": name,
        "pkg_version": version,
        "pkg_language": "javascript"
    });
    if !license.is_empty() {
        obj["pkg_license"] = serde_json::Value::String(license.to_string());
    }
    if !author.is_empty() {
        obj["pkg_author"] = serde_json::Value::String(author.to_string());
    }
    if !keywords.is_empty() {
        obj["pkg_keywords"] = serde_json::json!(keywords);
    }
    if !homepage.is_empty() {
        obj["pkg_homepage"] = serde_json::Value::String(homepage.to_string());
    }
    if let Some(r) = repo_url {
        obj["pkg_repo_url"] = serde_json::Value::String(r.to_string());
    }
    obj
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "package" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    // Reconstruct package name: supports scoped @scope/name
    let pkg_name = if segs.len() >= 3 && segs[1].starts_with('@') {
        format!("{}/{}", segs[1], segs[2])
    } else {
        segs[1].to_string()
    };

    // Encode scoped packages for URL: @scope/name → @scope%2Fname
    let encoded_name = pkg_name.replace('/', "%2F");
    let api_url = format!("https://registry.npmjs.org/{encoded_name}");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .header("Accept", "application/json")
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

    let data: serde_json::Value =
        resp.json()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            })?;

    let name = data["name"].as_str().unwrap_or(&pkg_name);
    let description = data["description"].as_str().unwrap_or("");
    let latest_version = data["dist-tags"]["latest"].as_str().unwrap_or("unknown");
    let license = data["license"].as_str().unwrap_or("");
    let homepage = data["homepage"].as_str().unwrap_or("");
    let author = extract_author(&data);
    let keywords: Vec<&str> = data["keywords"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let readme = data["readme"].as_str().unwrap_or("");
    let repo_url = extract_repo_url(&data);
    let engines = &data["versions"][latest_version]["engines"];

    let mut follow_crawl_urls: Vec<String> = vec![];
    if !homepage.is_empty() {
        follow_crawl_urls.push(homepage.to_string());
    }
    if let Some(ref r) = repo_url {
        follow_crawl_urls.push(r.clone());
    }

    let title = Some(format!("{name}@{latest_version}"));
    let md = build_npm_markdown(&NpmMarkdownData {
        name,
        latest_version,
        description,
        author: &author,
        license,
        homepage,
        repo_url: repo_url.as_deref(),
        keywords: &keywords,
        engines,
        readme,
        url,
    });

    let extra = build_extra(
        name,
        latest_version,
        license,
        &author,
        &keywords,
        homepage,
        repo_url.as_deref(),
    );

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls,
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "npm_tests.rs"]
mod tests;
