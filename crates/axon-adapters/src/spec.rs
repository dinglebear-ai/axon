//! Source-family matrix contract types.

use axon_api::source::{CredentialRequirement, SourceKind, SourceScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceFamily {
    Local,
    Upload,
    Git,
    Web,
    Deepwiki,
    Feed,
    Youtube,
    Reddit,
    Sessions,
    Registry,
    CliTool,
    McpTool,
    Memory,
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

/// Per-scope capability declaration, matching the 17-field contract shape in
/// `docs/pipeline-unification/sources/adapter-scopes.md#capability-shape`.
///
/// Field lineage for the two pre-existing fields kept for backward
/// compatibility with generated-doc consumers (`xtask/src/schemas/adapters.rs`):
/// `scope` fills the contract's `name` (the `SourceScope` enum already encodes
/// to the same snake_case tag `scope_feature_tag` uses), and `notes` fills the
/// contract's `description`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceScopeCapability {
    /// Scope name. Contract field: `name`.
    pub scope: SourceScope,
    /// Whether this scope must exist for the adapter to satisfy the family
    /// matrix contract (matrix-internal bookkeeping, not itself a contract
    /// field).
    pub required: bool,
    /// Human/agent-readable meaning. Contract field: `description`.
    pub notes: &'static str,
    /// Whether a run of this scope writes vectors by default.
    pub embeds_by_default: bool,
    /// Whether `watch` can keep this scope fresh.
    pub watch_supported: bool,
    /// Whether `refresh` applies to this scope.
    pub refresh_supported: bool,
    /// Whether credentials may be needed to acquire this scope.
    pub requires_credentials: bool,
    /// Whether this scope reads the local filesystem.
    pub may_access_local_paths: bool,
    /// Whether this scope performs network fetches.
    pub may_perform_network_fetches: bool,
    /// Whether this scope may invoke a browser/render provider.
    pub may_call_render_provider: bool,
    /// Whether this scope may execute CLI/MCP tools.
    pub may_execute_tools: bool,
    /// Whether this scope accepts prepared/staged uploads as input.
    pub accepts_uploads: bool,
    /// Output item kind produced by this scope (`file`, `page`, `repo_file`,
    /// `package`, `transcript`, `tool_call`, etc).
    pub output_item_kind: &'static str,
    /// JSON schema identifier for scope-specific options.
    pub option_schema: &'static str,
    /// Default chunk profile / parser hint for this scope.
    pub chunking_hints: &'static str,
    /// Graph facts this scope must emit when the source contains the
    /// corresponding structures.
    pub required_graph_fact_kinds: &'static [&'static str],
    /// Graph facts this scope may opportunistically emit without failing
    /// when absent.
    pub optional_graph_fact_kinds: &'static [&'static str],
    /// Allowed degraded behaviors for this scope.
    pub degraded_modes: &'static [&'static str],
}

/// Const constructor for [`SourceScopeCapability`] literals so the
/// family-matrix scope tables stay dense despite the 17-field contract shape.
#[allow(clippy::too_many_arguments)]
pub const fn scope_capability(
    scope: SourceScope,
    required: bool,
    notes: &'static str,
    embeds_by_default: bool,
    watch_supported: bool,
    refresh_supported: bool,
    requires_credentials: bool,
    may_access_local_paths: bool,
    may_perform_network_fetches: bool,
    may_call_render_provider: bool,
    may_execute_tools: bool,
    accepts_uploads: bool,
    output_item_kind: &'static str,
    option_schema: &'static str,
    chunking_hints: &'static str,
    required_graph_fact_kinds: &'static [&'static str],
    optional_graph_fact_kinds: &'static [&'static str],
    degraded_modes: &'static [&'static str],
) -> SourceScopeCapability {
    SourceScopeCapability {
        scope,
        required,
        notes,
        embeds_by_default,
        watch_supported,
        refresh_supported,
        requires_credentials,
        may_access_local_paths,
        may_perform_network_fetches,
        may_call_render_provider,
        may_execute_tools,
        accepts_uploads,
        output_item_kind,
        option_schema,
        chunking_hints,
        required_graph_fact_kinds,
        optional_graph_fact_kinds,
        degraded_modes,
    }
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

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
