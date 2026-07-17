//! Structured security audit event DTOs.
//!
//! Mirrors the "Audit Events" and "SSRF Policy" sections of
//! `docs/pipeline-unification/runtime/security-contract.md`. These are
//! transport-neutral data contracts only — emission/sink wiring lives in
//! `axon-observe`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::auth::AuthScope;
use super::ids::{JobId, SourceId, Timestamp};

/// The security-relevant event kinds enumerated by the "Audit Events"
/// section of the security contract. Exactly nine kinds — do not add more
/// without updating the contract doc first.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityAuditEventKind {
    /// auth denied
    AuthDenied,
    /// SSRF denied
    SsrfDenied,
    /// local path denied
    LocalPathDenied,
    /// tool execution denied
    ToolExecutionDenied,
    /// redaction failure
    RedactionFailure,
    /// secret detected and dropped
    SecretDetectedDropped,
    /// artifact traversal attempt
    ArtifactTraversalAttempt,
    /// destructive prune approved/executed
    DestructivePruneAction,
    /// credential missing/degraded
    CredentialDegraded,
}

/// Outcome of a policy check that produced an audit event. Used both
/// standalone (e.g. `DestructivePruneAction`) and inside detail payloads
/// like [`SsrfAuditDetail`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPolicyDecision {
    Allow,
    Deny,
}

/// Policy boundary that produced a non-SSRF security decision.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityDecisionBoundary {
    Authorization,
    LocalPath,
    CliToolExecution,
    McpToolExecution,
}

/// Typed, redaction-safe detail for authorization, local, and tool decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityDecisionAuditDetail {
    pub boundary: SecurityDecisionBoundary,
    pub policy_decision: SecurityPolicyDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_scope: Option<AuthScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Coarse classification of a resolved IP address, per the SSRF policy's
/// default-deny ranges.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedIpClass {
    Public,
    Loopback,
    LinkLocal,
    Private,
    UniqueLocal,
    Unspecified,
    /// Host was not a literal IP and no resolution was performed for this record.
    NotResolved,
}

/// Per-fetch SSRF audit detail. The contract requires every fetched URL to
/// record: requested URL, canonical URL, resolved IP class, redirect chain
/// position, policy decision, and a redacted-headers indicator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SsrfAuditDetail {
    pub requested_url: String,
    pub canonical_url: String,
    pub resolved_ip_class: ResolvedIpClass,
    /// Position of this URL in the redirect chain (0 = the original request).
    pub redirect_chain_index: u32,
    pub policy_decision: SecurityPolicyDecision,
    /// True when this fetch carried request headers that were redacted
    /// before being recorded anywhere (never raw header values in the audit
    /// record itself).
    pub headers_redacted: bool,
}

/// A structured, redaction-safe security audit record.
///
/// Per contract: "Audit events include `job_id`, caller identity when known,
/// source id when known, policy id/version, and redacted reason."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityAuditEvent {
    pub event_id: String,
    pub timestamp: Timestamp,
    pub kind: SecurityAuditEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<String>,
    /// Redacted human-readable reason. Must never contain secret values,
    /// raw header contents, or unredacted local paths.
    pub reason: String,
    /// Populated for authorization, local-path, and CLI/MCP tool policy
    /// decisions. Targets must already be redacted identifiers, never raw
    /// paths, argv, environment values, or tool output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<SecurityDecisionAuditDetail>,
    /// Populated when `kind == SsrfDenied` (or an SSRF allow-exception is
    /// recorded); `None` for all other event kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssrf: Option<SsrfAuditDetail>,
}

impl SecurityAuditEvent {
    /// Build a minimal audit event with a freshly generated `event_id` and
    /// current timestamp. Callers set `job_id`/`caller_id`/`source_id` and
    /// detail payloads afterward.
    pub fn new(kind: SecurityAuditEventKind, reason: impl Into<String>) -> Self {
        Self {
            event_id: format!("sec_{}", uuid::Uuid::new_v4()),
            timestamp: Timestamp::from(chrono::Utc::now()),
            kind,
            job_id: None,
            caller_id: None,
            source_id: None,
            policy_id: None,
            policy_version: None,
            reason: reason.into(),
            decision: None,
            ssrf: None,
        }
    }

    pub fn with_ssrf_detail(mut self, detail: SsrfAuditDetail) -> Self {
        self.ssrf = Some(detail);
        self
    }

    pub fn with_decision_detail(mut self, detail: SecurityDecisionAuditDetail) -> Self {
        self.decision = Some(detail);
        self
    }

    pub fn with_caller_id(mut self, caller_id: impl Into<String>) -> Self {
        self.caller_id = Some(caller_id.into());
        self
    }

    pub fn with_job_id(mut self, job_id: JobId) -> Self {
        self.job_id = Some(job_id);
        self
    }

    pub fn with_source_id(mut self, source_id: SourceId) -> Self {
        self.source_id = Some(source_id);
        self
    }

    pub fn with_policy(
        mut self,
        policy_id: impl Into<String>,
        policy_version: impl Into<String>,
    ) -> Self {
        self.policy_id = Some(policy_id.into());
        self.policy_version = Some(policy_version.into());
        self
    }
}
