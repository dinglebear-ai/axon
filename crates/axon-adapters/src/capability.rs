//! Adapter capability declarations.

use axon_api::source::*;

pub type AdapterVersion = String;

/// crate that owns the [`AdapterCapability`] -> [`SourceAdapterCapability`]
/// conversion, stamped into `CapabilityBase::owner_crate`.
const OWNER_CRATE: &str = "axon-adapters";

#[derive(Debug, Clone, PartialEq)]
pub struct AdapterCapability {
    pub adapter: AdapterRef,
    pub source_kind: SourceKind,
    pub default_scope: SourceScope,
    pub scopes: Vec<SourceScope>,
    pub credential_requirements: Vec<CredentialRequirement>,
    pub watch_supported: bool,
    pub refresh_supported: bool,
}

impl AdapterCapability {
    pub fn new(adapter: AdapterRef, source_kind: SourceKind, default_scope: SourceScope) -> Self {
        Self {
            adapter,
            source_kind,
            default_scope,
            scopes: vec![default_scope],
            credential_requirements: Vec::new(),
            watch_supported: true,
            refresh_supported: true,
        }
    }

    pub fn with_scope(mut self, scope: SourceScope) -> Self {
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
        self
    }

    pub fn validate_scope(&self, scope: SourceScope) -> Result<(), ApiError> {
        if self.scopes.contains(&scope) {
            return Ok(());
        }
        Err(ApiError::new(
            "adapter.scope.unsupported",
            axon_error::ErrorStage::Routing,
            "adapter does not support requested acquisition scope",
        )
        .with_context("adapter", self.adapter.name.clone())
        .with_context("scope", format!("{scope:?}")))
    }
}

/// Encodes a [`SourceScope`] as a `scope:<snake_case>` feature tag.
///
/// Uses the DTO's own `#[serde(rename_all = "snake_case")]` encoding so the
/// tag always matches the wire representation, e.g. `SourceScope::PullRequest`
/// -> `scope:pull_request`.
fn scope_feature_tag(scope: SourceScope) -> String {
    let encoded = serde_json::to_value(scope)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{scope:?}").to_lowercase());
    format!("scope:{encoded}")
}

/// Boundary conversion from the adapter crate's rich internal
/// [`AdapterCapability`] to the generic transport-neutral
/// `axon-api` [`SourceAdapterCapability`] the [`crate::adapter::SourceAdapter`]
/// trait returns.
///
/// `AdapterCapability`'s structured fields (`scopes`, `credential_requirements`,
/// `watch_supported`, `refresh_supported`) are not natively representable in
/// the generic `CapabilityBase { name, version, owner_crate, health, features,
/// limits }` shape, so they are re-encoded: supported scopes and
/// watch/refresh support become `features` tags, and the full structured
/// detail (source kind, default scope, scopes, credential requirements) is
/// packed as JSON into `limits` for callers that need it back. This is lossy
/// only in *shape*, not in *content* — `AdapterCapability` itself is
/// untouched and remains the crate's internal source of truth.
impl From<AdapterCapability> for SourceAdapterCapability {
    fn from(capability: AdapterCapability) -> Self {
        let mut features: Vec<String> = capability
            .scopes
            .iter()
            .copied()
            .map(scope_feature_tag)
            .collect();
        if capability.watch_supported {
            features.push("watch".to_string());
        }
        if capability.refresh_supported {
            features.push("refresh".to_string());
        }

        let mut limits = MetadataMap::new();
        limits.0.insert(
            "source_kind".to_string(),
            serde_json::json!(capability.source_kind),
        );
        limits.0.insert(
            "default_scope".to_string(),
            serde_json::json!(capability.default_scope),
        );
        limits
            .0
            .insert("scopes".to_string(), serde_json::json!(capability.scopes));
        limits.0.insert(
            "credential_requirements".to_string(),
            serde_json::json!(capability.credential_requirements),
        );

        CapabilityBase {
            name: capability.adapter.name,
            version: capability.adapter.version,
            owner_crate: OWNER_CRATE.to_string(),
            health: HealthStatus::Healthy,
            features,
            limits,
        }
        .into()
    }
}
