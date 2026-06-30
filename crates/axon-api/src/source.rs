//! Transport-neutral source pipeline DTOs.
//!
//! This module is the first narrow spike for the clean-break source pipeline.
//! It is data-only on purpose: CLI, MCP, REST, jobs, watches, and future
//! adapters should all be able to map into these shapes without pulling in
//! runtime crates.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceIntent {
    #[default]
    Acquire,
    Refresh,
    Watch,
    Map,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRefreshPolicy {
    #[default]
    IfStale,
    Force,
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceWatchPolicy {
    #[default]
    Disabled,
    Ensure,
    Enabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Foreground,
    #[default]
    Background,
    Wait,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    #[default]
    Auto,
    Summary,
    Full,
    Inline,
    Artifact,
    Path,
    JobOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Web,
    Local,
    Git,
    Registry,
    Feed,
    Reddit,
    Youtube,
    Session,
    CliTool,
    McpTool,
    Memory,
    Upload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScope {
    Page,
    Site,
    Docs,
    Repo,
    Workspace,
    Branch,
    Org,
    Package,
    Version,
    Feed,
    Subreddit,
    Thread,
    Comment,
    Video,
    Playlist,
    Channel,
    Issue,
    PullRequest,
    MergeRequest,
    Release,
    Wiki,
    File,
    Directory,
    Map,
    Tool,
    Script,
    Api,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRequest {
    pub source: String,
    #[serde(default)]
    pub intent: SourceIntent,
    #[serde(default = "default_embed")]
    pub embed: bool,
    #[serde(default)]
    pub refresh: SourceRefreshPolicy,
    #[serde(default)]
    pub watch: SourceWatchPolicy,
    #[serde(default)]
    pub execution: ExecutionMode,
    #[serde(default)]
    pub output: ResponseMode,
    #[serde(default)]
    pub limits: SourceLimits,
    #[serde(default)]
    pub options: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_hint: Option<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl SourceRequest {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            intent: SourceIntent::Acquire,
            embed: true,
            refresh: SourceRefreshPolicy::IfStale,
            watch: SourceWatchPolicy::Disabled,
            execution: ExecutionMode::Background,
            output: ResponseMode::Auto,
            limits: SourceLimits::default(),
            options: Map::new(),
            scope: None,
            collection: None,
            adapter: None,
            authority_hint: None,
            metadata: Map::new(),
        }
    }

    pub fn local_path(path: impl Into<String>, is_dir: bool) -> Self {
        let mut request = Self::new(path);
        request.scope = Some(if is_dir {
            SourceScope::Directory
        } else {
            SourceScope::File
        });
        request.adapter = Some("local".to_string());
        request
    }

    pub fn with_watch(mut self, watch: SourceWatchPolicy) -> Self {
        self.watch = watch;
        if watch != SourceWatchPolicy::Disabled {
            self.intent = SourceIntent::Watch;
        }
        self
    }

    pub fn with_refresh(mut self, refresh: SourceRefreshPolicy) -> Self {
        self.refresh = refresh;
        if refresh == SourceRefreshPolicy::Force {
            self.intent = SourceIntent::Refresh;
        }
        self
    }

    pub fn without_embedding(mut self) -> Self {
        self.embed = false;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSource {
    pub source: String,
    pub canonical_uri: String,
    pub source_kind: SourceKind,
    pub adapter: String,
    pub default_scope: SourceScope,
    pub available_scopes: Vec<SourceScope>,
    pub authority: String,
    pub confidence: f64,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

fn default_embed() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_file_request_serializes_to_canonical_source_shape() {
        let request = SourceRequest::local_path("/tmp/example.md", false);
        let value = match serde_json::to_value(&request) {
            Ok(value) => value,
            Err(err) => panic!("serialize source request: {err}"),
        };

        assert_eq!(value["source"], "/tmp/example.md");
        assert_eq!(value["intent"], "acquire");
        assert_eq!(value["embed"], true);
        assert_eq!(value["refresh"], "if_stale");
        assert_eq!(value["watch"], "disabled");
        assert_eq!(value["execution"], "background");
        assert_eq!(value["output"], "auto");
        assert_eq!(value["scope"], "file");
        assert_eq!(value["adapter"], "local");
    }

    #[test]
    fn local_directory_watch_request_uses_watch_intent_without_changing_embed_default() {
        let request = SourceRequest::local_path("/workspace/axon", true)
            .with_watch(SourceWatchPolicy::Ensure);

        assert_eq!(request.intent, SourceIntent::Watch);
        assert_eq!(request.watch, SourceWatchPolicy::Ensure);
        assert_eq!(request.scope, Some(SourceScope::Directory));
        assert!(request.embed);
    }

    #[test]
    fn source_request_deserializes_with_defaults_for_minimal_input() {
        let request: SourceRequest = match serde_json::from_value(json!({ "source": "shadcn.com" }))
        {
            Ok(request) => request,
            Err(err) => panic!("deserialize source request: {err}"),
        };

        assert_eq!(request.source, "shadcn.com");
        assert_eq!(request.intent, SourceIntent::Acquire);
        assert_eq!(request.refresh, SourceRefreshPolicy::IfStale);
        assert_eq!(request.watch, SourceWatchPolicy::Disabled);
        assert_eq!(request.execution, ExecutionMode::Background);
        assert!(request.embed);
        assert!(request.options.is_empty());
        assert!(request.metadata.is_empty());
    }

    #[test]
    fn force_refresh_sets_refresh_intent() {
        let request =
            SourceRequest::new("github.com/jmagar/axon").with_refresh(SourceRefreshPolicy::Force);

        assert_eq!(request.intent, SourceIntent::Refresh);
        assert_eq!(request.refresh, SourceRefreshPolicy::Force);
    }
}
