//! Source-family matrix contract types.

use axon_api::source::{CredentialRequirement, SourceKind, SourceScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceFamily {
    Local,
    Git,
    Web,
    Feed,
    Youtube,
    Reddit,
    Sessions,
    Registry,
    CliTool,
    McpTool,
    MemoryIntegration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserFamily {
    None,
    Markdown,
    Html,
    Code,
    Manifest,
    Feed,
    Transcript,
    Session,
    PackageMetadata,
    ToolOutput,
    ApiSchema,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceScopeCapability {
    pub scope: SourceScope,
    pub required: bool,
    pub notes: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAdapterSpec {
    pub family: SourceFamily,
    pub adapter: &'static str,
    pub version: &'static str,
    pub source_kinds: &'static [SourceKind],
    pub vector_namespace: &'static str,
    pub supported_schemes: &'static [&'static str],
    pub shorthand_patterns: &'static [&'static str],
    pub default_scope: SourceScope,
    pub scopes: &'static [SourceScopeCapability],
    pub credential_requirements: &'static [CredentialRequirement],
    pub option_schema: &'static str,
    pub parser_families: &'static [ParserFamily],
    pub metadata_families: &'static [&'static str],
    pub watch_supported: bool,
    pub refresh_supported: bool,
    pub may_access_local_paths: bool,
    pub may_perform_network_fetches: bool,
    pub may_call_render_provider: bool,
    pub may_execute_tools: bool,
    pub is_source_adapter: bool,
    pub degraded_modes: &'static [&'static str],
    pub required_graph_fact_kinds: &'static [&'static str],
    pub optional_graph_fact_kinds: &'static [&'static str],
}

impl SourceAdapterSpec {
    pub fn public_resolver_family(&self) -> bool {
        self.is_source_adapter
    }
}
