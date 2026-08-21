//! Opt-in, hard-bounded skills.sh security-audit enrichment.

use async_trait::async_trait;
use axon_api::source::{ApiError, ErrorStage};
use axon_core::http::build_client;
use axon_error::{RetryPolicy, RetryScope};
use futures_util::StreamExt;
use url::Url;

use crate::adapter::Result;

use super::{SkillsShAudit, SkillsShAuditResponse, SkillsShDump};

const API_BASE: &str = "https://skills.sh/api/v1/skills";
const USER_AGENT: &str = "axon-artifact-catalog/1.0 (+https://github.com/dinglebear-ai/axon)";
const REQUEST_TIMEOUT_SECS: u64 = 20;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_RETRY_AFTER_SECS: u64 = 300;
const MAX_AUDITS_PER_SKILL: usize = 16;
const MAX_PROVIDER_LEN: usize = 128;
const MAX_SLUG_LEN: usize = 128;
const MAX_SUMMARY_LEN: usize = 1024;
const MAX_TIMESTAMP_LEN: usize = 64;
const MAX_RISK_LEVEL_LEN: usize = 16;
const MAX_CATEGORIES: usize = 16;
const MAX_CATEGORY_LEN: usize = 64;

#[async_trait]
pub(crate) trait SkillsShAuditProvider: Send + Sync {
    async fn fetch_audit(&self, id: &str) -> Result<Option<SkillsShAuditResponse>>;
}

#[derive(Clone)]
struct HttpSkillsShAuditProvider {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl HttpSkillsShAuditProvider {
    fn new(base_url: String, token: String) -> Result<Self> {
        super::fetch::validate_token(&token)?;
        let client = build_client(REQUEST_TIMEOUT_SECS, Some(USER_AGENT)).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.audit_client_init_failed",
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
        Self::new(API_BASE.to_string(), super::fetch::oidc_token()?)
    }
}

#[async_trait]
impl SkillsShAuditProvider for HttpSkillsShAuditProvider {
    async fn fetch_audit(&self, id: &str) -> Result<Option<SkillsShAuditResponse>> {
        let url = audit_url(&self.base_url, id)?;
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| retryable_audit_error("audit_fetch_failed", error.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(rate_limit_error(&response));
        }
        if matches!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) {
            return Err(ApiError::new(
                "adapter.skills_sh.audit_auth_failed",
                ErrorStage::Fetching,
                "skills.sh rejected the configured bearer token while fetching audit metadata",
            ));
        }
        if response.status().is_server_error() {
            return Err(retryable_audit_error(
                "audit_unavailable",
                format!(
                    "skills.sh audit API returned HTTP {}",
                    response.status().as_u16()
                ),
            ));
        }
        if !response.status().is_success() {
            return Err(ApiError::new(
                "adapter.skills_sh.audit_http_error",
                ErrorStage::Fetching,
                format!(
                    "skills.sh audit API returned HTTP {}",
                    response.status().as_u16()
                ),
            ));
        }
        let bytes = read_bounded_body(response).await?;
        serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.audit_response_invalid",
                ErrorStage::Fetching,
                format!("skills.sh returned invalid audit JSON: {error}"),
            )
        })
    }
}

pub(crate) async fn enrich_dump(dump: &mut SkillsShDump, audit_limit: usize) {
    let selected = audit_limit.min(dump.skills.len());
    if selected == 0 {
        return;
    }
    match HttpSkillsShAuditProvider::production() {
        Ok(provider) => enrich_dump_with_provider(dump, selected, &provider).await,
        Err(error) => mark_audit_failure(dump, 0, selected, &error.code.0),
    }
}

pub(crate) async fn enrich_dump_with_provider(
    dump: &mut SkillsShDump,
    audit_limit: usize,
    provider: &dyn SkillsShAuditProvider,
) {
    let selected = audit_limit.min(dump.skills.len());
    for index in 0..selected {
        let id = dump.skills[index].id.clone();
        match provider.fetch_audit(&id).await {
            Ok(Some(response)) => match validate_response(&dump.skills[index], response) {
                Ok(audits) => {
                    dump.skills[index].audit_status = Some(if audits.is_empty() {
                        "none".to_string()
                    } else {
                        "available".to_string()
                    });
                    dump.skills[index].audits = audits;
                }
                Err(error) => {
                    mark_audit_failure(dump, index, selected, &error.code.0);
                    break;
                }
            },
            Ok(None) => {
                dump.skills[index].audit_status = Some("none".to_string());
                dump.skills[index].audits.clear();
            }
            Err(error) => {
                mark_audit_failure(dump, index, selected, &error.code.0);
                break;
            }
        }
    }
}

fn mark_audit_failure(dump: &mut SkillsShDump, failed_index: usize, selected: usize, code: &str) {
    dump.skills[failed_index].audit_status = Some("unavailable".to_string());
    dump.skills[failed_index]
        .audit_warnings
        .push(format!("skills.sh audit enrichment stopped: {code}"));
    for skill in &mut dump.skills[failed_index.saturating_add(1)..selected] {
        skill.audit_status = Some("skipped_after_failure".to_string());
    }
}

fn validate_response(
    skill: &super::SkillsShSkill,
    response: SkillsShAuditResponse,
) -> Result<Vec<SkillsShAudit>> {
    if response.id != skill.id || response.source != skill.source || response.slug != skill.slug {
        return Err(ApiError::new(
            "adapter.skills_sh.audit_identity_mismatch",
            ErrorStage::Fetching,
            "skills.sh audit response identity did not match the requested listing item",
        ));
    }
    if response.audits.len() > MAX_AUDITS_PER_SKILL {
        return Err(audit_shape_error("too many audit partner entries"));
    }
    let mut audits = response.audits;
    for audit in &audits {
        validate_audit(audit)?;
    }
    audits.sort_by(|left, right| {
        left.slug
            .cmp(&right.slug)
            .then_with(|| left.provider.cmp(&right.provider))
    });
    Ok(audits)
}

fn validate_audit(audit: &SkillsShAudit) -> Result<()> {
    bounded_nonempty(&audit.provider, MAX_PROVIDER_LEN, "provider")?;
    bounded_nonempty(&audit.slug, MAX_SLUG_LEN, "slug")?;
    bounded_nonempty(&audit.summary, MAX_SUMMARY_LEN, "summary")?;
    bounded_nonempty(&audit.audited_at, MAX_TIMESTAMP_LEN, "auditedAt")?;
    if !matches!(audit.status.as_str(), "pass" | "warn" | "fail") {
        return Err(audit_shape_error("unsupported audit status"));
    }
    chrono::DateTime::parse_from_rfc3339(&audit.audited_at)
        .map_err(|_| audit_shape_error("audit timestamp is not RFC3339"))?;
    if let Some(risk) = audit.risk_level.as_deref() {
        bounded_nonempty(risk, MAX_RISK_LEVEL_LEN, "riskLevel")?;
        if !matches!(risk, "NONE" | "LOW" | "MEDIUM" | "HIGH" | "CRITICAL") {
            return Err(audit_shape_error("unsupported audit risk level"));
        }
    }
    if audit.categories.len() > MAX_CATEGORIES {
        return Err(audit_shape_error("too many audit categories"));
    }
    for category in &audit.categories {
        bounded_nonempty(category, MAX_CATEGORY_LEN, "category")?;
    }
    Ok(())
}

fn bounded_nonempty(value: &str, max: usize, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(audit_shape_error(&format!(
            "audit {field} must be non-empty and at most {max} bytes"
        )));
    }
    Ok(())
}

fn audit_shape_error(message: &str) -> ApiError {
    ApiError::new(
        "adapter.skills_sh.audit_shape_invalid",
        ErrorStage::Fetching,
        message,
    )
}

fn retryable_audit_error(suffix: &str, message: String) -> ApiError {
    ApiError::new(
        format!("adapter.skills_sh.{suffix}"),
        ErrorStage::Fetching,
        message,
    )
    .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider))
    .with_provider_id("skills.sh")
}

fn rate_limit_error(response: &reqwest::Response) -> ApiError {
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.min(MAX_RETRY_AFTER_SECS));
    let mut error = retryable_audit_error(
        "audit_rate_limited",
        "skills.sh audit API returned HTTP 429; optional audit enrichment stopped".to_string(),
    );
    if let Some(seconds) = retry_after {
        error = error
            .with_retry_after_ms(seconds.saturating_mul(1_000))
            .with_context("retry_after_seconds", seconds.to_string());
    }
    error
}

async fn read_bounded_body(response: reqwest::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(audit_shape_error("audit response exceeds byte cap"));
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| retryable_audit_error("audit_body_read_failed", error.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(audit_shape_error("audit response exceeds byte cap"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn audit_url(base_url: &str, id: &str) -> Result<Url> {
    let segments = id.split('/').collect::<Vec<_>>();
    if segments.len() < 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || matches!(*segment, "." | "..")
                || segment.len() > 256
                || segment
                    .chars()
                    .any(|ch| ch.is_control() || matches!(ch, '\\' | '?' | '#'))
        })
    {
        return Err(ApiError::new(
            "adapter.skills_sh.audit_id_invalid",
            ErrorStage::Fetching,
            "skills.sh listing id cannot be represented as a safe audit path",
        ));
    }
    let mut url = Url::parse(base_url).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.audit_url_invalid",
            ErrorStage::Fetching,
            error.to_string(),
        )
    })?;
    url.set_query(None);
    url.set_fragment(None);
    {
        let mut path = url.path_segments_mut().map_err(|_| {
            ApiError::new(
                "adapter.skills_sh.audit_url_invalid",
                ErrorStage::Fetching,
                "skills.sh audit base URL cannot accept path segments",
            )
        })?;
        path.pop_if_empty().push("audit");
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

#[cfg(test)]
pub(crate) fn http_provider_for_test(
    base_url: Url,
    token: &str,
) -> Result<impl SkillsShAuditProvider> {
    HttpSkillsShAuditProvider::new(base_url.to_string(), token.to_string())
}

#[cfg(test)]
pub(crate) fn audit_url_for_test(base_url: &str, id: &str) -> Result<Url> {
    audit_url(base_url, id)
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
