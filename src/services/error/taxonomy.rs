//! Vertical / antibot / structured-extraction error taxonomy.
//!
//! Each variant of [`ServiceTaxonomyError`] maps to a stable machine-readable
//! MCP error code so agents can branch on retry strategy without parsing
//! human-readable messages. The wire contract is documented in
//! `docs/MCP-TOOL-SCHEMA.md`.

use serde_json::{Value, json};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};
use std::time::Duration;

/// Antibot challenge vendor detected on a page.
///
/// Used by [`ServiceTaxonomyError::ChallengeDetected`] and
/// [`ServiceTaxonomyError::VerticalBlockedAntibot`] so MCP agents can branch on
/// the specific provider (different cooldowns, different recovery strategies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeVendor {
    Cloudflare,
    DataDome,
    AwsWaf,
    HCaptcha,
    Akamai,
    Other(&'static str),
}

impl ChallengeVendor {
    /// Machine-readable vendor identifier emitted in MCP error details.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChallengeVendor::Cloudflare => "cloudflare",
            ChallengeVendor::DataDome => "datadome",
            ChallengeVendor::AwsWaf => "aws_waf",
            ChallengeVendor::HCaptcha => "hcaptcha",
            ChallengeVendor::Akamai => "akamai",
            ChallengeVendor::Other(s) => s,
        }
    }
}

impl Display for ChallengeVendor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured taxonomy of vertical-extractor, antibot, and structured-data
/// failures. Each variant maps to a stable machine-readable MCP error code
/// (see [`ServiceTaxonomyError::mcp_code`]) so agents can branch on retry
/// strategy without parsing human-readable messages.
///
/// This sits alongside the existing [`super::ServiceError`] struct: callers
/// that need a typed retry signal construct one of these variants and bubble
/// it up via `Box<dyn std::error::Error>`. The MCP boundary uses
/// [`taxonomy_from_error`] to downcast and emit the structured response.
#[derive(Debug, Clone)]
pub enum ServiceTaxonomyError {
    /// Antibot interstitial detected. `recoverable=true` means the page may be
    /// reachable after a cookie-warmup retry; `false` means do not retry on
    /// this URL until backoff expires.
    ChallengeDetected {
        vendor: ChallengeVendor,
        recoverable: bool,
        retry_after: Option<Duration>,
    },
    /// Vertical extractor was rate-limited by the upstream API (HTTP 429 or
    /// equivalent). `retry_after` is the upstream `Retry-After` hint if any.
    VerticalRateLimited {
        vertical: &'static str,
        retry_after: Option<Duration>,
    },
    /// Vertical requires credentials that are not configured (e.g. missing
    /// `GITHUB_TOKEN` for `github_repo`).
    VerticalAuthMissing { vertical: &'static str },
    /// Configured credentials were rejected by the upstream API (401/403).
    VerticalAuthInvalid { vertical: &'static str },
    /// The URL does not match the patterns this vertical handles. The caller
    /// should fall back to the generic crawl/scrape path.
    VerticalUnsupportedUrl { vertical: &'static str, url: String },
    /// Vertical reached the upstream API but the target resource does not
    /// exist (HTTP 404). Not retriable.
    VerticalTargetNotFound { vertical: &'static str, url: String },
    /// Upstream returned a 5xx that may resolve on its own. Caller may retry
    /// with backoff.
    VerticalTargetUnavailable { vertical: &'static str, status: u16 },
    /// Vertical reached the target but was blocked by an antibot vendor —
    /// distinct from `ChallengeDetected` because the failure surfaced through
    /// the vertical's structured response rather than the page body.
    VerticalBlockedAntibot {
        vertical: &'static str,
        vendor: ChallengeVendor,
    },
    /// A structured-data fragment (`jsonld`, `next_data`, `sveltekit`) was
    /// present but could not be parsed. Not retriable — the page itself is
    /// the source of truth and the extractor should fall back to markdown.
    StructuredDataMalformed {
        source: &'static str,
        reason: String,
    },
    /// All escalation steps in the retry ladder failed. `final_word_count` is
    /// the largest payload any step produced — useful for diagnostics when a
    /// page legitimately has very little content vs. being blocked.
    LadderExhausted { final_word_count: usize },
}

impl ServiceTaxonomyError {
    /// Machine-readable MCP error code (snake_case). Stable wire identifier
    /// for agents to branch on retry strategy.
    pub fn mcp_code(&self) -> &'static str {
        match self {
            Self::ChallengeDetected { .. } => "challenge_detected",
            Self::VerticalRateLimited { .. } => "vertical_rate_limited",
            Self::VerticalAuthMissing { .. } => "vertical_auth_missing",
            Self::VerticalAuthInvalid { .. } => "vertical_auth_invalid",
            Self::VerticalUnsupportedUrl { .. } => "vertical_unsupported_url",
            Self::VerticalTargetNotFound { .. } => "vertical_target_not_found",
            Self::VerticalTargetUnavailable { .. } => "vertical_target_unavailable",
            Self::VerticalBlockedAntibot { .. } => "vertical_blocked_antibot",
            Self::StructuredDataMalformed { .. } => "structured_data_malformed",
            Self::LadderExhausted { .. } => "ladder_exhausted",
        }
    }

    /// Whether an agent should retry this operation. Encodes the locked MCP
    /// contract — see `docs/MCP-TOOL-SCHEMA.md`.
    pub fn retriable(&self) -> bool {
        match self {
            Self::ChallengeDetected { recoverable, .. } => *recoverable,
            Self::VerticalRateLimited { .. } => true,
            Self::VerticalTargetUnavailable { .. } => true,
            Self::VerticalBlockedAntibot { .. } => true,
            Self::VerticalAuthMissing { .. }
            | Self::VerticalAuthInvalid { .. }
            | Self::VerticalUnsupportedUrl { .. }
            | Self::VerticalTargetNotFound { .. }
            | Self::StructuredDataMalformed { .. }
            | Self::LadderExhausted { .. } => false,
        }
    }

    /// Source identifier for the MCP error envelope. For vertical errors this
    /// is the vertical name (e.g. `github_repo`); for antibot/structured
    /// errors it identifies the detector or parser.
    pub fn mcp_source(&self) -> &'static str {
        match self {
            Self::ChallengeDetected { .. } => "antibot",
            Self::VerticalRateLimited { vertical, .. }
            | Self::VerticalAuthMissing { vertical }
            | Self::VerticalAuthInvalid { vertical }
            | Self::VerticalUnsupportedUrl { vertical, .. }
            | Self::VerticalTargetNotFound { vertical, .. }
            | Self::VerticalTargetUnavailable { vertical, .. }
            | Self::VerticalBlockedAntibot { vertical, .. } => vertical,
            Self::StructuredDataMalformed { source, .. } => source,
            Self::LadderExhausted { .. } => "extractor_ladder",
        }
    }

    /// Per-variant details object for the MCP error envelope. Stable shape
    /// per code — agents can rely on these field names.
    pub fn mcp_details(&self) -> Value {
        match self {
            Self::ChallengeDetected {
                vendor,
                recoverable,
                retry_after,
            } => json!({
                "vendor": vendor.as_str(),
                "recoverable": recoverable,
                "retry_after_secs": retry_after.map(|d| d.as_secs()),
            }),
            Self::VerticalRateLimited {
                vertical,
                retry_after,
            } => json!({
                "vertical": vertical,
                "retry_after_secs": retry_after.map(|d| d.as_secs()),
            }),
            Self::VerticalAuthMissing { vertical } | Self::VerticalAuthInvalid { vertical } => {
                json!({ "vertical": vertical })
            }
            Self::VerticalUnsupportedUrl { vertical, url }
            | Self::VerticalTargetNotFound { vertical, url } => json!({
                "vertical": vertical,
                "url": url,
            }),
            Self::VerticalTargetUnavailable { vertical, status } => json!({
                "vertical": vertical,
                "status": status,
            }),
            Self::VerticalBlockedAntibot { vertical, vendor } => json!({
                "vertical": vertical,
                "vendor": vendor.as_str(),
            }),
            Self::StructuredDataMalformed { source, reason } => json!({
                "source": source,
                "reason": reason,
            }),
            Self::LadderExhausted { final_word_count } => json!({
                "final_word_count": final_word_count,
            }),
        }
    }

    /// Build the complete MCP error envelope for this variant. The shape
    /// matches the contract documented in `docs/MCP-TOOL-SCHEMA.md`:
    ///
    /// ```json
    /// { "error": { "code": "...", "retriable": true, "source": "...", "details": {...} } }
    /// ```
    pub fn to_mcp_envelope(&self) -> Value {
        json!({
            "error": {
                "code": self.mcp_code(),
                "retriable": self.retriable(),
                "source": self.mcp_source(),
                "details": self.mcp_details(),
            }
        })
    }
}

impl Display for ServiceTaxonomyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChallengeDetected {
                vendor,
                recoverable,
                retry_after,
            } => {
                write!(
                    f,
                    "antibot challenge detected (vendor={vendor}, recoverable={recoverable}"
                )?;
                if let Some(d) = retry_after {
                    write!(f, ", retry_after={}s", d.as_secs())?;
                }
                write!(f, ")")
            }
            Self::VerticalRateLimited {
                vertical,
                retry_after,
            } => {
                write!(f, "{vertical} rate-limited by upstream")?;
                if let Some(d) = retry_after {
                    write!(f, " (retry_after={}s)", d.as_secs())?;
                }
                Ok(())
            }
            Self::VerticalAuthMissing { vertical } => {
                write!(f, "{vertical} requires credentials (none configured)")
            }
            Self::VerticalAuthInvalid { vertical } => {
                write!(f, "{vertical} credentials rejected by upstream")
            }
            Self::VerticalUnsupportedUrl { vertical, url } => {
                write!(f, "url not handled by {vertical} extractor: {url}")
            }
            Self::VerticalTargetNotFound { vertical, url } => {
                write!(f, "{vertical} target not found: {url}")
            }
            Self::VerticalTargetUnavailable { vertical, status } => {
                write!(f, "{vertical} target unavailable (status={status})")
            }
            Self::VerticalBlockedAntibot { vertical, vendor } => {
                write!(f, "{vertical} blocked by antibot ({vendor})")
            }
            Self::StructuredDataMalformed { source, reason } => {
                write!(f, "{source} structured data malformed: {reason}")
            }
            Self::LadderExhausted { final_word_count } => {
                write!(
                    f,
                    "extractor ladder exhausted (final_word_count={final_word_count})"
                )
            }
        }
    }
}

impl StdError for ServiceTaxonomyError {}

/// Walk an error/source chain and return the first [`ServiceTaxonomyError`]
/// encountered. MCP handlers call this at the response boundary to convert
/// taxonomy errors into the structured `{ error: { code, retriable, ... } }`
/// envelope; falls back to `internal_error` mapping when no taxonomy variant
/// is present.
pub fn taxonomy_from_error<'a>(
    err: &'a (dyn StdError + 'static),
) -> Option<&'a ServiceTaxonomyError> {
    let mut cursor = Some(err);
    while let Some(current) = cursor {
        if let Some(tax) = current.downcast_ref::<ServiceTaxonomyError>() {
            return Some(tax);
        }
        cursor = current.source();
    }
    None
}

#[cfg(test)]
#[path = "taxonomy_tests.rs"]
mod tests;
