use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::core::config::Config;

use super::types::{GitLabProject, GitLabTarget};

pub(crate) fn build_gitlab_client(cfg: &Config) -> Result<Client> {
    let mut headers = HeaderMap::new();
    if let Some(token) = cfg
        .gitlab_token
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        headers.insert("PRIVATE-TOKEN", HeaderValue::from_str(token)?);
    }
    Ok(Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

pub(crate) async fn fetch_project(client: &Client, target: &GitLabTarget) -> Result<GitLabProject> {
    Ok(client
        .get(target.project_api_url(""))
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
            pairs.append_pair("per_page", "100");
            pairs.append_pair("page", &page.to_string());
        }
        let response = client.get(request_url).send().await?.error_for_status()?;
        let next_page = response
            .headers()
            .get("x-next-page")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok());
        let mut page_items: Vec<T> = response.json().await?;
        if max_items > 0 {
            let remaining = max_items.saturating_sub(out.len());
            page_items.truncate(remaining);
        }
        out.append(&mut page_items);
        if max_items > 0 && out.len() >= max_items {
            break;
        }
        let Some(next) = next_page.filter(|next| *next > page) else {
            break;
        };
        page = next;
    }
    Ok(out)
}
