use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{CallerContext, TransportKind};
use super::enums::Visibility;
use super::ids::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthScope {
    Read,
    Write,
    Admin,
    Execute,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    TrustedLocal,
    StaticToken,
    Oauth,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub transport: TransportKind,
    pub granted_scopes: Vec<AuthScope>,
    pub visibility_ceiling: Visibility,
    pub request_time: Timestamp,
    pub policy_version: String,
    pub auth_mode: AuthMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl AuthSnapshot {
    pub fn trusted_system(policy_version: impl Into<String>) -> Self {
        Self {
            caller_id: Some("axon-system".to_string()),
            transport: TransportKind::System,
            granted_scopes: vec![AuthScope::Read, AuthScope::Write, AuthScope::Admin],
            visibility_ceiling: Visibility::Internal,
            request_time: Timestamp::from(Utc::now()),
            policy_version: policy_version.into(),
            auth_mode: AuthMode::TrustedLocal,
            token_id: None,
            display_name: Some("Axon system".to_string()),
        }
    }

    pub fn from_caller(
        caller: &CallerContext,
        visibility_ceiling: Visibility,
        policy_version: impl Into<String>,
    ) -> Self {
        Self {
            caller_id: caller.caller_id.clone(),
            transport: caller.transport,
            granted_scopes: caller
                .scopes
                .iter()
                .filter_map(|scope| AuthScope::from_scope_str(scope))
                .collect(),
            visibility_ceiling,
            request_time: Timestamp::from(Utc::now()),
            policy_version: policy_version.into(),
            auth_mode: caller.auth_mode.clone(),
            token_id: caller.token_id.clone(),
            display_name: caller.display_name.clone(),
        }
    }
}

impl Default for AuthSnapshot {
    fn default() -> Self {
        Self {
            caller_id: None,
            transport: TransportKind::System,
            granted_scopes: vec![AuthScope::Read, AuthScope::Write],
            visibility_ceiling: Visibility::Internal,
            request_time: Timestamp::from(Utc::now()),
            policy_version: "test".to_string(),
            auth_mode: AuthMode::Test,
            token_id: None,
            display_name: None,
        }
    }
}

impl AuthScope {
    pub fn from_scope_str(scope: &str) -> Option<Self> {
        match scope {
            "axon:read" | "source:read" | "read" => Some(Self::Read),
            "axon:write" | "source:write" | "write" => Some(Self::Write),
            "axon:admin" | "admin" => Some(Self::Admin),
            "axon:execute" | "execute" => Some(Self::Execute),
            "axon:local" | "local" => Some(Self::Local),
            _ => None,
        }
    }
}
