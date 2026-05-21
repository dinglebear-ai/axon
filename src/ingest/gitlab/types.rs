use std::error::Error;

use anyhow::{Result, anyhow, bail};
use reqwest::Url;
use serde::Deserialize;

use crate::core::http::validate_url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabTarget {
    pub host: String,
    pub namespace_path: String,
    pub project: String,
    pub web_url: String,
    pub clone_url: String,
    pub api_base: String,
    pub encoded_project_path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabProject {
    pub path_with_namespace: String,
    pub name: String,
    pub description: Option<String>,
    pub default_branch: Option<String>,
    pub web_url: String,
    pub visibility: Option<String>,
    pub star_count: Option<u64>,
    pub forks_count: Option<u64>,
    pub open_issues_count: Option<u64>,
    pub issues_enabled: Option<bool>,
    pub merge_requests_enabled: Option<bool>,
    pub wiki_enabled: Option<bool>,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabUser {
    pub username: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabIssue {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: Option<String>,
    pub web_url: Option<String>,
    pub author: Option<GitLabUser>,
    pub labels: Option<Vec<String>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub user_notes_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabMergeRequest {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: Option<String>,
    pub web_url: Option<String>,
    pub author: Option<GitLabUser>,
    pub labels: Option<Vec<String>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub user_notes_count: Option<u64>,
    pub merged_at: Option<String>,
    pub draft: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabWikiPage {
    pub slug: String,
    pub title: String,
    pub content: Option<String>,
    pub format: Option<String>,
    pub encoding: Option<String>,
}

pub fn normalize_gitlab_target(input: &str) -> std::result::Result<String, Box<dyn Error>> {
    Ok(parse_gitlab_target(input)?.as_normalized_target())
}

pub fn parse_gitlab_target(input: &str) -> Result<GitLabTarget> {
    let raw = input.trim();
    let raw = raw.strip_prefix("gitlab:").unwrap_or(raw).trim();
    let parsed = if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)?
    } else {
        Url::parse(&format!("https://{raw}"))?
    };

    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        bail!("invalid GitLab target '{input}': expected https URL");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("invalid GitLab target '{input}': missing host"))?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let mut segments: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if let Some(marker) = segments.iter().position(|part| *part == "-") {
        segments.truncate(marker);
    }
    if segments.len() < 2 {
        bail!("invalid GitLab target '{input}': expected host/group/project");
    }
    let mut parts: Vec<String> = segments.into_iter().map(str::to_string).collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".git").to_string();
    }
    let project = parts
        .last()
        .filter(|part| !part.is_empty())
        .cloned()
        .ok_or_else(|| anyhow!("invalid GitLab target '{input}': missing project"))?;
    let namespace_path = parts.join("/");
    let web_url = format!("https://{host}/{namespace_path}");
    let clone_url = format!("{web_url}.git");
    validate_url(&web_url)?;
    validate_url(&clone_url)?;
    let encoded_project_path = percent_encode_path(&namespace_path);
    Ok(GitLabTarget {
        host: host.clone(),
        namespace_path,
        project,
        web_url,
        clone_url,
        api_base: format!("https://{host}/api/v4"),
        encoded_project_path,
    })
}

impl GitLabTarget {
    pub(crate) fn as_normalized_target(&self) -> String {
        format!("{}/{}", self.host, self.namespace_path)
    }

    pub(crate) fn project_api_url(&self, suffix: &str) -> String {
        format!(
            "{}/projects/{}{}",
            self.api_base, self.encoded_project_path, suffix
        )
    }
}

fn percent_encode_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
