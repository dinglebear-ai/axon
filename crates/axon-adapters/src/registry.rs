//! Source adapter registry.

use std::sync::Arc;

use axon_api::source::*;
use axon_error::ErrorStage;

use crate::adapter::SourceAdapter;

#[derive(Clone, Default)]
pub struct SourceAdapterRegistry {
    adapters: Vec<Arc<dyn SourceAdapter>>,
}

impl SourceAdapterRegistry {
    pub fn from_adapters<A>(adapters: Vec<A>) -> Self
    where
        A: SourceAdapter + 'static,
    {
        Self::from_arc_adapters(
            adapters
                .into_iter()
                .map(|adapter| Arc::new(adapter) as Arc<dyn SourceAdapter>)
                .collect(),
        )
    }

    pub fn from_arc_adapters(mut adapters: Vec<Arc<dyn SourceAdapter>>) -> Self {
        adapters.sort_by(|left, right| left.name().cmp(right.name()));
        Self { adapters }
    }

    pub fn from_boxed_adapters(adapters: Vec<Box<dyn SourceAdapter>>) -> Self {
        Self::from_arc_adapters(adapters.into_iter().map(Arc::from).collect())
    }

    pub fn adapter_for(&self, route: &RoutePlan) -> Option<Arc<dyn SourceAdapter>> {
        self.adapter_for_source_kind(route.source.source_kind)
    }

    pub fn adapter_for_source_kind(
        &self,
        source_kind: SourceKind,
    ) -> Option<Arc<dyn SourceAdapter>> {
        crate::source_family_matrix()
            .iter()
            .find(|spec| {
                spec.is_source_adapter && spec.source_kinds.first().copied() == Some(source_kind)
            })
            .and_then(|spec| {
                self.adapters
                    .iter()
                    .find(|adapter| adapter.name() == spec.adapter)
            })
            .cloned()
    }

    /// Validate the concrete registry against the normative family matrix.
    ///
    /// This is intentionally explicit and asynchronous: capability metadata
    /// is part of the adapter boundary, so startup must validate the same
    /// values that transports and the runner will observe rather than a
    /// parallel declaration maintained by the registry.
    pub async fn validate(&self) -> Result<(), ApiError> {
        let mut names = std::collections::BTreeSet::new();
        for adapter in &self.adapters {
            let name = adapter.name();
            if !names.insert(name) {
                return Err(registry_error(
                    "adapter.registry.duplicate",
                    format!("duplicate source adapter name: {name}"),
                ));
            }
        }

        for spec in crate::source_family_matrix() {
            if !spec.is_source_adapter {
                continue;
            }
            let adapter = self
                .adapters
                .iter()
                .find(|candidate| candidate.name() == spec.adapter)
                .ok_or_else(|| {
                    registry_error(
                        "adapter.registry.missing",
                        format!("family {:?} requires adapter {}", spec.family, spec.adapter),
                    )
                })?;
            if adapter.version() != spec.version {
                return Err(registry_error(
                    "adapter.registry.version_mismatch",
                    format!(
                        "adapter {} reports {}, matrix requires {}",
                        spec.adapter,
                        adapter.version(),
                        spec.version
                    ),
                ));
            }
            let capability = adapter.capabilities().await?.0;
            validate_capability(spec, &capability)?;
        }
        Ok(())
    }
}

fn registry_error(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Planning, message)
}

pub(crate) fn validate_capability(
    spec: &crate::SourceAdapterSpec,
    capability: &CapabilityBase,
) -> Result<(), ApiError> {
    let source_kind = capability
        .limits
        .0
        .get("source_kind")
        .and_then(|value| serde_json::from_value::<SourceKind>(value.clone()).ok());
    if !source_kind.is_some_and(|kind| spec.source_kinds.contains(&kind)) {
        return Err(registry_error(
            "adapter.registry.capability_mismatch",
            format!(
                "adapter {} reports a source kind outside family {:?}",
                spec.adapter, spec.family
            ),
        ));
    }
    let default_scope = capability
        .limits
        .0
        .get("default_scope")
        .and_then(|value| serde_json::from_value::<SourceScope>(value.clone()).ok());
    if default_scope != Some(spec.default_scope) {
        return Err(registry_error(
            "adapter.registry.capability_mismatch",
            format!("adapter {} reports the wrong default scope", spec.adapter),
        ));
    }
    for scope in spec.scopes {
        if scope.required {
            let tag = format!(
                "scope:{}",
                serde_json::to_value(scope.scope)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default()
            );
            if !capability.features.iter().any(|feature| feature == &tag) {
                return Err(registry_error(
                    "adapter.registry.capability_mismatch",
                    format!(
                        "adapter {} is missing required scope {:?}",
                        spec.adapter, scope.scope
                    ),
                ));
            }
        }
    }
    Ok(())
}
