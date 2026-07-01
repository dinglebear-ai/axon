//! Adapter declarations consumed by the router.

use axon_api::{
    AdapterRef, ChunkHint, CredentialKind, CredentialRequirement, ExecutionAffinity, ParserHint,
    ProviderRequirement, SafetyClass, SourceKind, SourceScope,
};

#[derive(Debug, Clone, PartialEq)]
pub struct AdapterDefinition {
    pub adapter: AdapterRef,
    pub source_kind: SourceKind,
    pub default_scope: SourceScope,
    pub supported_scopes: Vec<SourceScope>,
    pub safety_class: SafetyClass,
    pub execution_affinity: ExecutionAffinity,
    pub provider_requirements: Vec<ProviderRequirement>,
    pub credential_requirements: Vec<CredentialRequirement>,
    pub option_schema_id: String,
    pub allowed_option_keys: Vec<String>,
    pub chunking_hints: Vec<ChunkHint>,
    pub parser_hints: Vec<ParserHint>,
    pub watch_supported: bool,
    pub refresh_supported: bool,
}

impl AdapterDefinition {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        source_kind: SourceKind,
        default_scope: SourceScope,
    ) -> Self {
        let name = name.into();
        Self {
            adapter: AdapterRef {
                name: name.clone(),
                version: version.into(),
            },
            source_kind,
            default_scope,
            supported_scopes: vec![default_scope],
            safety_class: safety_class(source_kind),
            execution_affinity: ExecutionAffinity::Worker,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            option_schema_id: format!("adapter:{name}:options:v1"),
            allowed_option_keys: Vec::new(),
            chunking_hints: Vec::new(),
            parser_hints: Vec::new(),
            watch_supported: matches!(source_kind, SourceKind::Local | SourceKind::Web),
            refresh_supported: true,
        }
    }

    pub fn with_scope(mut self, scope: SourceScope) -> Self {
        if !self.supported_scopes.contains(&scope) {
            self.supported_scopes.push(scope);
        }
        self
    }

    pub fn with_safety_class(mut self, safety_class: SafetyClass) -> Self {
        self.safety_class = safety_class;
        self
    }

    pub fn with_credential(mut self, credential_kind: CredentialKind, reason: &str) -> Self {
        self.credential_requirements.push(CredentialRequirement {
            credential_kind,
            secret_ref: None,
            required: true,
            reason: reason.to_string(),
        });
        self
    }
}

fn safety_class(source_kind: SourceKind) -> SafetyClass {
    match source_kind {
        SourceKind::Local => SafetyClass::LocalFilesystem,
        SourceKind::CliTool | SourceKind::McpTool => SafetyClass::ToolExecution,
        _ => SafetyClass::PublicNetwork,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AdapterRegistry {
    adapters: Vec<AdapterDefinition>,
}

impl AdapterRegistry {
    pub fn from_adapters(mut adapters: Vec<AdapterDefinition>) -> Self {
        for adapter in &mut adapters {
            if let Some(minimum) = minimum_safety_class(adapter.source_kind) {
                if safety_rank(adapter.safety_class) < safety_rank(minimum) {
                    adapter.safety_class = minimum;
                }
            }
            if !adapter.supported_scopes.contains(&adapter.default_scope) {
                adapter.supported_scopes.push(adapter.default_scope);
            }
        }
        adapters.sort_by(|left, right| left.adapter.name.cmp(&right.adapter.name));
        Self { adapters }
    }

    pub fn target_defaults() -> Self {
        Self::from_adapters(vec![
            AdapterDefinition::new("cli", "1", SourceKind::CliTool, SourceScope::Tool)
                .with_safety_class(SafetyClass::ToolExecution),
            AdapterDefinition::new("crates", "1", SourceKind::Registry, SourceScope::Package)
                .with_scope(SourceScope::Version),
            AdapterDefinition::new("docker", "1", SourceKind::Registry, SourceScope::Package)
                .with_scope(SourceScope::Version),
            AdapterDefinition::new("feed", "1", SourceKind::Feed, SourceScope::Feed),
            AdapterDefinition::new("github", "1", SourceKind::Git, SourceScope::Repo)
                .with_scope(SourceScope::Branch)
                .with_scope(SourceScope::Issue)
                .with_scope(SourceScope::PullRequest)
                .with_scope(SourceScope::Release),
            AdapterDefinition::new("git", "1", SourceKind::Git, SourceScope::Repo)
                .with_scope(SourceScope::Branch),
            AdapterDefinition::new("gitea", "1", SourceKind::Git, SourceScope::Repo)
                .with_scope(SourceScope::Branch)
                .with_scope(SourceScope::Issue)
                .with_scope(SourceScope::PullRequest)
                .with_scope(SourceScope::Release),
            AdapterDefinition::new("gitlab", "1", SourceKind::Git, SourceScope::Repo)
                .with_scope(SourceScope::Branch)
                .with_scope(SourceScope::Issue)
                .with_scope(SourceScope::MergeRequest)
                .with_scope(SourceScope::Release),
            AdapterDefinition::new("local", "1", SourceKind::Local, SourceScope::Directory)
                .with_scope(SourceScope::File)
                .with_scope(SourceScope::Workspace)
                .with_safety_class(SafetyClass::LocalFilesystem),
            AdapterDefinition::new("mcp", "1", SourceKind::McpTool, SourceScope::Tool)
                .with_safety_class(SafetyClass::ToolExecution),
            AdapterDefinition::new("npm", "1", SourceKind::Registry, SourceScope::Package)
                .with_scope(SourceScope::Version),
            AdapterDefinition::new("pypi", "1", SourceKind::Registry, SourceScope::Package)
                .with_scope(SourceScope::Version),
            AdapterDefinition::new("reddit", "1", SourceKind::Reddit, SourceScope::Subreddit)
                .with_scope(SourceScope::Thread)
                .with_scope(SourceScope::Comment)
                .with_safety_class(SafetyClass::AuthenticatedNetwork)
                .with_credential(
                    CredentialKind::ApiKey,
                    "Reddit API credentials are required before acquisition",
                ),
            AdapterDefinition::new("session", "1", SourceKind::Session, SourceScope::Thread),
            AdapterDefinition::new("upload", "1", SourceKind::Upload, SourceScope::File),
            AdapterDefinition::new("web", "1", SourceKind::Web, SourceScope::Site)
                .with_scope(SourceScope::Page)
                .with_scope(SourceScope::Docs)
                .with_scope(SourceScope::Map),
            AdapterDefinition::new("youtube", "1", SourceKind::Youtube, SourceScope::Video)
                .with_scope(SourceScope::Playlist)
                .with_scope(SourceScope::Channel),
        ])
    }

    pub fn adapters_for(&self, source_kind: SourceKind) -> Vec<&AdapterDefinition> {
        self.adapters
            .iter()
            .filter(|adapter| adapter.source_kind == source_kind)
            .collect()
    }

    pub fn find(&self, name: &str) -> Option<&AdapterDefinition> {
        self.adapters
            .iter()
            .find(|adapter| adapter.adapter.name == name)
    }

    pub fn definitions(&self) -> &[AdapterDefinition] {
        &self.adapters
    }
}

fn minimum_safety_class(source_kind: SourceKind) -> Option<SafetyClass> {
    match source_kind {
        SourceKind::Local | SourceKind::CliTool | SourceKind::McpTool => {
            Some(safety_class(source_kind))
        }
        _ => None,
    }
}

fn safety_rank(safety_class: SafetyClass) -> u8 {
    match safety_class {
        SafetyClass::PublicNetwork => 0,
        SafetyClass::AuthenticatedNetwork => 1,
        SafetyClass::LocalFilesystem => 2,
        SafetyClass::ToolExecution => 3,
    }
}
