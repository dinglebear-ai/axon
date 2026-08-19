//! Bounded structured skills.sh API acquisition.

use std::collections::BTreeMap;

use async_trait::async_trait;
use axon_api::source::{ApiError, ErrorStage, SourcePlan, Timestamp};
use axon_core::http::build_client;
use axon_error::{RetryPolicy, RetryScope};
use futures_util::StreamExt;
use url::Url;

use crate::adapter::Result;

use super::{SkillsShDump, SkillsShMode, SkillsShOptions, SkillsShPage, options};

const API_BASE: &str = "https://skills.sh/api/v1/skills";
const USER_AGENT: &str = "axon-artifact-catalog/1.0 (+https://github.com/dinglebear-ai/axon)";
const REQUEST_TIMEOUT_SECS: u64 = 20;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RETRY_AFTER_SECS: u64 = 300;
const PRIMARY_TOKEN_ENV: &str = "SKILLS_SH_OIDC_TOKEN";
const VERCEL_TOKEN_ENV: &str = "VERCEL_OIDC_TOKEN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillsShPageRequest {
    pub(crate) mode: SkillsShMode,
    pub(crate) view: super::SkillsShView,
    pub(crate) query: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) page: u32,
    pub(crate) limit: u32,
}

#[async_trait]
pub(crate) trait SkillsShPageProvider: Send + Sync {
    async fn fetch_page(&self, request: SkillsShPageRequest) -> Result<SkillsShPage>;
}

#[derive(Clone)]
pub(crate) struct HttpSkillsShPageProvider {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl HttpSkillsShPageProvider {
    fn new(base_url: String, token: String) -> Result<Self> {
        let client = build_client(REQUEST_TIMEOUT_SECS, Some(USER_AGENT)).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.client_init_failed",
                ErrorStage::Fetching,
                error.to_string(),
            )
        })?;
        Ok(Self {
            client,
            base_url,
            token,
        })
    }

    fn production() -> Result<Self> {
        Self::new(API_BASE.to_string(), oidc_token()?)
    }
}

#[async_trait]
impl SkillsShPageProvider for HttpSkillsShPageProvider {
    async fn fetch_page(&self, request: SkillsShPageRequest) -> Result<SkillsShPage> {
        let url = request_url(&self.base_url, &request)?;
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| {
                ApiError::new(
                    "adapter.skills_sh.fetch_failed",
                    ErrorStage::Fetching,
                    format!("skills.sh API request failed: {error}"),
                )
                .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider))
                .with_provider_id("skills.sh")
            })?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|seconds| seconds.min(MAX_RETRY_AFTER_SECS));
            let mut error = ApiError::new(
                "adapter.skills_sh.rate_limited",
                ErrorStage::Fetching,
                "skills.sh API returned HTTP 429; stop this bounded run and retry later",
            )
            .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider))
            .with_provider_id("skills.sh");
            if let Some(seconds) = retry_after {
                error = error
                    .with_retry_after_ms(seconds.saturating_mul(1_000))
                    .with_context("retry_after_seconds", seconds.to_string());
            }
            return Err(error);
        }
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ApiError::new(
                "adapter.skills_sh.auth_failed",
                ErrorStage::Fetching,
                "skills.sh rejected the configured Vercel OIDC bearer token",
            ));
        }
        if response.status().is_server_error() {
            return Err(ApiError::new(
                "adapter.skills_sh.unavailable",
                ErrorStage::Fetching,
                format!("skills.sh API returned HTTP {}", response.status().as_u16()),
            )
            .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider))
            .with_provider_id("skills.sh"));
        }
        if !response.status().is_success() {
            return Err(ApiError::new(
                "adapter.skills_sh.http_error",
                ErrorStage::Fetching,
                format!("skills.sh API returned HTTP {}", response.status().as_u16()),
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES)
        {
            return Err(ApiError::new(
                "adapter.skills_sh.response_too_large",
                ErrorStage::Fetching,
                format!("skills.sh response exceeds {MAX_RESPONSE_BYTES} byte cap"),
            ));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                ApiError::new(
                    "adapter.skills_sh.body_read_failed",
                    ErrorStage::Fetching,
                    error.to_string(),
                )
            })?;
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES as usize {
                return Err(ApiError::new(
                    "adapter.skills_sh.response_too_large",
                    ErrorStage::Fetching,
                    format!("skills.sh response exceeds {MAX_RESPONSE_BYTES} byte cap"),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.response_invalid",
                ErrorStage::Fetching,
                format!("skills.sh returned invalid JSON: {error}"),
            )
        })
    }
}

pub(crate) async fn fetch_dump_to_temporary_file(
    plan: &SourcePlan,
) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
    let options = options(plan)?;
    let provider = HttpSkillsShPageProvider::production()?;
    let mut dump = fetch_dump(&provider, &options).await?;
    if options.audit_limit > 0 {
        super::audit::enrich_dump(&mut dump, options.audit_limit).await;
    }
    let temporary = tempfile::tempdir().map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.tempdir_failed",
            ErrorStage::Fetching,
            error.to_string(),
        )
    })?;
    let path = temporary.path().join("skills-sh.json");
    let bytes = serde_json::to_vec(&dump).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.dump_serialize_failed",
            ErrorStage::Fetching,
            error.to_string(),
        )
    })?;
    std::fs::write(&path, bytes).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.dump_write_failed",
            ErrorStage::Fetching,
            error.to_string(),
        )
    })?;
    Ok((temporary, path))
}

pub(crate) async fn fetch_dump(
    provider: &dyn SkillsShPageProvider,
    options: &SkillsShOptions,
) -> Result<SkillsShDump> {
    let mut skills = BTreeMap::new();
    let mut pages_fetched = 0_u32;
    let mut total_reported = None;
    match options.mode {
        SkillsShMode::Search => {
            let page = provider
                .fetch_page(SkillsShPageRequest {
                    mode: SkillsShMode::Search,
                    view: options.view,
                    query: options.query.clone(),
                    owner: options.owner.clone(),
                    page: 0,
                    limit: options.per_page,
                })
                .await?;
            extend_unique(&mut skills, page.data, options.total_limit)?;
            pages_fetched = 1;
        }
        SkillsShMode::Leaderboard => {
            for offset in 0..options.max_pages {
                if skills.len() >= options.total_limit {
                    break;
                }
                let page_number = options.start_page.saturating_add(offset);
                let page = provider
                    .fetch_page(SkillsShPageRequest {
                        mode: SkillsShMode::Leaderboard,
                        view: options.view,
                        query: None,
                        owner: None,
                        page: page_number,
                        limit: options.per_page,
                    })
                    .await?;
                pages_fetched = pages_fetched.saturating_add(1);
                if let Some(pagination) = &page.pagination {
                    validate_pagination(pagination, page_number, options.per_page)?;
                    total_reported = Some(pagination.total);
                }
                let has_more = page
                    .pagination
                    .as_ref()
                    .is_some_and(|pagination| pagination.has_more);
                extend_unique(&mut skills, page.data, options.total_limit)?;
                if !has_more {
                    break;
                }
            }
        }
    }
    Ok(SkillsShDump {
        provider: "skills.sh".to_string(),
        mode: match options.mode {
            SkillsShMode::Leaderboard => "leaderboard",
            SkillsShMode::Search => "search",
        }
        .to_string(),
        observed_at: Timestamp(chrono::Utc::now().to_rfc3339()),
        skills: skills.into_values().collect(),
        pages_fetched,
        total_reported,
    })
}

fn validate_pagination(
    pagination: &super::SkillsShPagination,
    requested_page: u32,
    requested_per_page: u32,
) -> Result<()> {
    if pagination.page != requested_page || pagination.per_page > requested_per_page {
        return Err(ApiError::new(
            "adapter.skills_sh.pagination_invalid",
            ErrorStage::Fetching,
            format!(
                "skills.sh pagination response did not match the bounded request: page={} perPage={} requested_page={requested_page} requested_per_page={requested_per_page}",
                pagination.page, pagination.per_page
            ),
        ));
    }
    Ok(())
}

fn extend_unique(
    skills: &mut BTreeMap<String, super::SkillsShSkill>,
    page: Vec<super::SkillsShSkill>,
    total_limit: usize,
) -> Result<()> {
    for skill in page {
        if skills.len() >= total_limit {
            break;
        }
        let skill = validate_and_sanitize_listing_skill(skill)?;
        skills.entry(skill.id.clone()).or_insert(skill);
    }
    Ok(())
}

fn validate_and_sanitize_listing_skill(
    mut skill: super::SkillsShSkill,
) -> Result<super::SkillsShSkill> {
    bounded_listing_text(&skill.id, 1024, "id")?;
    bounded_listing_text(&skill.slug, 256, "slug")?;
    bounded_listing_text(&skill.name, 512, "name")?;
    bounded_listing_text(&skill.source, 512, "source")?;
    bounded_listing_text(&skill.source_type, 32, "sourceType")?;
    if skill
        .slug
        .chars()
        .any(|ch| matches!(ch, '/' | '\\' | '?' | '#'))
        || matches!(skill.slug.as_str(), "." | "..")
    {
        return Err(listing_shape_error("slug is not a safe path segment"));
    }
    if skill.id != format!("{}/{}", skill.source, skill.slug) {
        return Err(listing_shape_error(
            "stable id must exactly match the documented {source}/{slug} shape",
        ));
    }
    match skill.source_type.as_str() {
        "github" if valid_github_source(&skill.source) => {}
        "well-known" if valid_well_known_source(&skill.source) => {}
        "github" | "well-known" => {
            return Err(listing_shape_error(
                "source does not match the documented sourceType shape",
            ));
        }
        _ => {
            return Err(listing_shape_error(
                "sourceType is not supported by the v1 listing API",
            ));
        }
    }
    for (field, value) in [("installUrl", &skill.install_url), ("url", &skill.url)] {
        if let Some(value) = value
            && (value.len() > 2048 || value.chars().any(char::is_control))
        {
            return Err(listing_shape_error(&format!(
                "{field} exceeds the bounded URL evidence shape"
            )));
        }
    }
    // These fields are Axon-owned enrichment. The listing/search API cannot
    // inject them and bypass the dedicated audit-response validator.
    skill.audits.clear();
    skill.audit_status = None;
    skill.audit_warnings.clear();
    Ok(skill)
}

fn bounded_listing_text(value: &str, max: usize, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(listing_shape_error(&format!(
            "listing {field} must be non-empty, control-free, and at most {max} bytes"
        )));
    }
    Ok(())
}

fn valid_github_source(source: &str) -> bool {
    let mut parts = source.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(owner), Some(repo), None)
            if valid_listing_segment(owner) && valid_listing_segment(repo)
    )
}

fn valid_well_known_source(source: &str) -> bool {
    !source.contains('/')
        && Url::parse(&format!("https://{source}/"))
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .is_some_and(|host| host.eq_ignore_ascii_case(source))
}

fn valid_listing_segment(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && !value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | '?' | '#'))
}

fn listing_shape_error(message: &str) -> ApiError {
    ApiError::new(
        "adapter.skills_sh.listing_invalid",
        ErrorStage::Fetching,
        message,
    )
}

fn request_url(base_url: &str, request: &SkillsShPageRequest) -> Result<Url> {
    let base_url = base_url.trim_end_matches('/');
    let raw_url = match request.mode {
        SkillsShMode::Leaderboard => base_url.to_string(),
        SkillsShMode::Search => format!("{base_url}/search"),
    };
    let mut url = Url::parse(&raw_url).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.url_invalid",
            ErrorStage::Fetching,
            error.to_string(),
        )
    })?;
    match request.mode {
        SkillsShMode::Leaderboard => {
            url.query_pairs_mut()
                .append_pair("view", request.view.as_api_value())
                .append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.limit.to_string());
        }
        SkillsShMode::Search => {
            url.query_pairs_mut()
                .append_pair("q", request.query.as_deref().unwrap_or_default())
                .append_pair("limit", &request.limit.to_string());
            if let Some(owner) = request.owner.as_deref() {
                url.query_pairs_mut().append_pair("owner", owner);
            }
        }
    }
    Ok(url)
}

pub(super) fn oidc_token() -> Result<String> {
    let token = std::env::var(PRIMARY_TOKEN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var(VERCEL_TOKEN_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| {
            ApiError::new(
                "adapter.skills_sh.credentials_missing",
                ErrorStage::Authorizing,
                "skills.sh API requires a Vercel OIDC bearer token; configure SKILLS_SH_OIDC_TOKEN or VERCEL_OIDC_TOKEN in the worker environment",
            )
        })?;
    Ok(token)
}

#[cfg(test)]
pub(crate) fn request_url_for_test(request: &SkillsShPageRequest) -> Result<Url> {
    request_url(API_BASE, request)
}

#[cfg(test)]
pub(crate) fn http_provider_for_test(
    base_url: Url,
    token: &str,
) -> Result<HttpSkillsShPageProvider> {
    HttpSkillsShPageProvider::new(base_url.to_string(), token.to_string())
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
