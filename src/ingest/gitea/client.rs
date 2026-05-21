use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::core::config::Config;

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaRepo {
    pub(crate) full_name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) clone_url: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) private: Option<bool>,
    pub(crate) stars_count: Option<u64>,
    pub(crate) forks_count: Option<u64>,
    pub(crate) open_issues_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaUser {
    pub(crate) login: Option<String>,
    pub(crate) full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaIssue {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) user: Option<GiteaUser>,
    pub(crate) labels: Option<Vec<GiteaLabel>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) comments: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaLabel {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaPullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) user: Option<GiteaUser>,
    pub(crate) labels: Option<Vec<GiteaLabel>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) comments: Option<u64>,
    pub(crate) merged: Option<bool>,
}

pub(crate) fn build_client(cfg: &Config) -> Result<Client> {
    let mut headers = HeaderMap::new();
    if let Some(token) = cfg.gitea_token.as_deref().filter(|token| !token.is_empty()) {
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}"))?,
        );
    }
    Ok(Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

pub(crate) async fn fetch_repo(client: &Client, target: &super::GiteaTarget) -> Result<GiteaRepo> {
    Ok(client
        .get(target.repo_api_url(""))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub(crate) async fn fetch_paginated<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    max_items: usize,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    let mut page = 1usize;
    loop {
        let mut request_url = Url::parse(url)?;
        {
            let mut pairs = request_url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
            pairs.append_pair("limit", "100");
            pairs.append_pair("page", &page.to_string());
        }
        let mut page_items: Vec<T> = client
            .get(request_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let page_len = page_items.len();
        if max_items > 0 {
            let remaining = max_items.saturating_sub(out.len());
            page_items.truncate(remaining);
        }
        out.append(&mut page_items);
        if page_len < 100 || (max_items > 0 && out.len() >= max_items) {
            break;
        }
        page += 1;
    }
    Ok(out)
}
