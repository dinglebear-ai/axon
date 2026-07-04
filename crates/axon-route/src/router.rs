//! Source router boundary.

use axon_api::{
    AdapterOptions, ChunkHint, ChunkProfile, ExecutionAffinity, ParserHint, ProviderRequirement,
    ResolvedSource, RoutePlan, SafetyClass, SourceKind, SourceRequest,
};
use axon_error::{ApiError, ErrorStage};

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
            parser_hints: parser_hints(adapter.source_kind),
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
        _ => ChunkProfile::MarkdownSections,
    };
    vec![ChunkHint {
        profile,
        reason: "route default chunk profile".to_string(),
        options: Default::default(),
    }]
}

fn parser_hints(source_kind: SourceKind) -> Vec<ParserHint> {
    vec![ParserHint {
        parser_id: source_kind_key(source_kind).to_string(),
        reason: "route default parser".to_string(),
        options: Default::default(),
    }]
}

fn source_kind_key(source_kind: SourceKind) -> &'static str {
    match source_kind {
        SourceKind::Web => "web",
        SourceKind::Local => "local",
        SourceKind::Git => "git",
        SourceKind::Registry => "registry",
        SourceKind::Feed => "feed",
        SourceKind::Reddit => "reddit",
        SourceKind::Youtube => "youtube",
        SourceKind::Session => "session",
        SourceKind::CliTool => "cli_tool",
        SourceKind::McpTool => "mcp_tool",
        SourceKind::Memory => "memory",
        SourceKind::Upload => "upload",
    }
}

fn graph_fact_kinds(source_kind: SourceKind) -> Vec<String> {
    match source_kind {
        SourceKind::Git => vec!["repo".to_string(), "package_manifest".to_string()],
        SourceKind::Registry => vec!["package".to_string()],
        SourceKind::CliTool | SourceKind::McpTool => vec!["tool".to_string()],
        _ => vec!["source".to_string()],
    }
}
