//! Adapter capability declarations.

use axon_api::source::*;

pub type AdapterVersion = String;

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
