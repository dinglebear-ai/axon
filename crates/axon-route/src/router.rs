//! Source router boundary.

use axon_api::{
    AdapterOptions, CapabilityBase, ChunkHint, ChunkProfile, ExecutionAffinity, HealthStatus,
    MetadataMap, ProviderRequirement, ResolvedSource, RoutePlan, SafetyClass, SourceKind,
    SourceRequest, SourceRouterCapability, ValidatedOptions,
};
use axon_error::{ApiError, ErrorStage};

use crate::boundary;
use crate::capability::{AdapterDefinition, AdapterRegistry};
use crate::source_id::source_id;

pub type RouteDecision = RoutePlan;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RouteSecurityPolicy {
    allow_tool_execution: bool,
}

impl RouteSecurityPolicy {
    pub fn trusted_tool_execution() -> Self {
        Self {
            allow_tool_execution: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceRouter {
    adapters: AdapterRegistry,
}

impl SourceRouter {
    pub fn new(adapters: AdapterRegistry) -> Self {
        Self { adapters }
    }

    pub fn route(
        &self,
        request: &SourceRequest,
        source: ResolvedSource,
    ) -> Result<RoutePlan, ApiError> {
        self.route_with_policy(request, source, RouteSecurityPolicy::default())
    }

    pub fn route_with_policy(
        &self,
        request: &SourceRequest,
        source: ResolvedSource,
        policy: RouteSecurityPolicy,
    ) -> Result<RoutePlan, ApiError> {
        self.validate_source(&source)?;
        let scope = request.scope.unwrap_or(source.default_scope);
        let adapter = self.select_adapter(request, &source)?;

        if !adapter.supported_scopes.contains(&scope) {
            return Err(ApiError::new(
                "source.scope.unsupported",
                ErrorStage::Routing,
                "adapter does not support requested source scope",
            )
            .with_context("adapter", adapter.adapter.name.clone())
            .with_context("scope", format!("{scope:?}")));
        }
        self.validate_options(request, adapter, policy)?;

        Ok(RoutePlan {
            source,
            adapter: adapter.adapter.clone(),
            scope,
            provider_requirements: provider_requirements(adapter),
            credential_requirements: adapter.credential_requirements.clone(),
            execution_affinity: execution_affinity(adapter),
            safety_class: adapter.safety_class,
            option_schema_id: adapter.option_schema_id.clone(),
            validated_options: AdapterOptions {
                values: request.options.values.clone(),
            },
            chunking_hints: chunking_hints(adapter.source_kind),
            parser_hints: adapter.parser_hints.clone(),
            graph_fact_kinds: graph_fact_kinds(adapter.source_kind),
            watch_supported: adapter.watch_supported,
            refresh_supported: adapter.refresh_supported,
        })
    }

    fn validate_source(&self, source: &ResolvedSource) -> Result<(), ApiError> {
        let expected = source_id(source.source_kind, &source.canonical_uri);
        if source.source_id != expected {
            return Err(ApiError::new(
                "route.source.invalid",
                ErrorStage::Routing,
                "resolved source id does not match canonical identity",
            ));
        }

        let Some(adapter) = self.adapters.find(&source.adapter.name) else {
            return Err(ApiError::new(
                "route.source.invalid",
                ErrorStage::Routing,
                "resolved source references an unregistered adapter",
            )
            .with_context("adapter", source.adapter.name.clone()));
        };
        if adapter.source_kind != source.source_kind {
            return Err(ApiError::new(
                "route.source.invalid",
                ErrorStage::Routing,
                "resolved source adapter does not match source kind",
            )
            .with_context("adapter", source.adapter.name.clone()));
        }

        Ok(())
    }

    fn select_adapter(
        &self,
        request: &SourceRequest,
        source: &ResolvedSource,
    ) -> Result<&AdapterDefinition, ApiError> {
        if let Some(name) = request.adapter.as_deref() {
            let adapter = self.adapters.find(name).ok_or_else(|| {
                ApiError::new(
                    "route.adapter.unknown",
                    ErrorStage::Routing,
                    "requested adapter is not registered",
                )
                .with_context("adapter", name.to_string())
            })?;
            if adapter.source_kind != source.source_kind {
                return Err(ApiError::new(
                    "route.adapter.unsupported_source",
                    ErrorStage::Routing,
                    "requested adapter does not support resolved source kind",
                )
                .with_context("adapter", name.to_string())
                .with_context("source_kind", format!("{:?}", source.source_kind)));
            }
            if source.adapter.name != name {
                return Err(ApiError::new(
                    "route.adapter.unsupported_source",
                    ErrorStage::Routing,
                    "requested adapter does not match resolved adapter",
                )
                .with_context("adapter", name.to_string())
                .with_context("resolved_adapter", source.adapter.name.clone())
                .with_context("canonical_uri", source.canonical_uri.clone()));
            }
            return Ok(adapter);
        }

        self.adapters.find(&source.adapter.name).ok_or_else(|| {
            ApiError::new(
                "route.adapter.missing",
                ErrorStage::Routing,
                "resolved adapter is not registered",
            )
            .with_context("adapter", source.adapter.name.clone())
        })
    }

    fn validate_options(
        &self,
        request: &SourceRequest,
        adapter: &AdapterDefinition,
        policy: RouteSecurityPolicy,
    ) -> Result<(), ApiError> {
        for key in request.options.values.keys() {
            if !adapter.allowed_option_keys.contains(key) {
                return Err(ApiError::new(
                    "route.options.unsupported",
                    ErrorStage::Routing,
                    "unsupported route option for selected adapter schema",
                )
                .with_context("adapter", adapter.adapter.name.clone())
                .with_context("option_schema_id", adapter.option_schema_id.clone())
                .with_context("option", key.clone()));
            }
        }

        // Web is the first adapter with a real (non-legacy) per-option value
        // schema (adapter-scopes.md "Web Adapter" table); see `web_options.rs`.
        // Other adapters only get the key-membership check above until they
        // grow their own typed option schemas.
        if adapter.adapter.name == "web" {
            crate::web_options::validate(&request.options.values)?;
        }

        if adapter.safety_class == SafetyClass::ToolExecution && !policy.allow_tool_execution {
            return Err(ApiError::new(
                "route.tool_execution.denied",
                ErrorStage::Routing,
                "tool execution sources require trusted execution policy",
            )
            .with_context("adapter", adapter.adapter.name.clone()));
        }

        Ok(())
    }
}

/// `boundary::SourceRouter` trait implementation for the concrete
/// `SourceRouter` struct.
///
/// `self.route(request, source)` below resolves to the pre-existing inherent
/// sync method (`impl SourceRouter { pub fn route(...) }` above) because
/// inherent methods always shadow same-named trait methods for direct
/// dot-call resolution in Rust — this is NOT recursion. The contract's
/// argument order (`source, request`) is reversed from the inherent method's
/// (`request, source`); that's fine because they're different items thanks
/// to the module split.
#[async_trait::async_trait]
impl boundary::SourceRouter for SourceRouter {
    async fn route(
        &self,
        source: ResolvedSource,
        request: &SourceRequest,
    ) -> boundary::Result<RoutePlan> {
        self.route(request, source)
    }

    // TODO(#298): this is a pass-through echo, not a real re-validation. A
    // `RoutePlan` cannot yield an invalid state (`route()` above already
    // errors before constructing one), and `RoutePlan` doesn't carry the
    // `RouteSecurityPolicy`/`AdapterDefinition` needed to re-run the real
    // option-key/tool-execution checks performed by the private
    // `validate_options` gate above. Flagged for the pipeline-unification
    // cutover to decide whether this method should become a genuine
    // idempotent re-check or be dropped from the contract.
    async fn validate_options(&self, plan: &RoutePlan) -> boundary::Result<ValidatedOptions> {
        Ok(ValidatedOptions {
            values: plan.validated_options.values.clone(),
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> boundary::Result<SourceRouterCapability> {
        Ok(SourceRouterCapability::from(CapabilityBase {
            name: "axon-route-router".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-route".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["adapter-selection".to_string(), "scope-routing".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

fn execution_affinity(adapter: &AdapterDefinition) -> ExecutionAffinity {
    if adapter.source_kind == SourceKind::CliTool || adapter.source_kind == SourceKind::McpTool {
        ExecutionAffinity::ProviderBound
    } else {
        adapter.execution_affinity
    }
}

fn provider_requirements(adapter: &AdapterDefinition) -> Vec<ProviderRequirement> {
    if !adapter.provider_requirements.is_empty() {
        return adapter.provider_requirements.clone();
    }
    Vec::new()
}

fn chunking_hints(source_kind: SourceKind) -> Vec<ChunkHint> {
    let profile = match source_kind {
        SourceKind::Git | SourceKind::Local => ChunkProfile::CodeSymbol,
        SourceKind::Session => ChunkProfile::SessionTurns,
        SourceKind::Youtube => ChunkProfile::TranscriptSegments,
        SourceKind::Registry | SourceKind::McpTool | SourceKind::CliTool | SourceKind::Upload => {
            ChunkProfile::StructuredRecords
        }
        SourceKind::Memory => ChunkProfile::AtomicMetadata,
        _ => ChunkProfile::MarkdownSections,
    };
    vec![ChunkHint {
        profile,
        reason: "route default chunk profile".to_string(),
        options: Default::default(),
    }]
}

fn graph_fact_kinds(source_kind: SourceKind) -> Vec<String> {
    match source_kind {
        SourceKind::Git => vec!["repo".to_string(), "package_manifest".to_string()],
        SourceKind::Registry => vec!["package".to_string()],
        SourceKind::CliTool | SourceKind::McpTool => vec!["tool".to_string()],
        SourceKind::Memory => vec!["memory".to_string()],
        _ => vec!["source".to_string()],
    }
}
