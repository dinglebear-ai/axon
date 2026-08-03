use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod pipeline_phase;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceIntent {
    #[default]
    Acquire,
    Refresh,
    Watch,
    Map,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceRefreshPolicy {
    #[default]
    IfStale,
    Force,
    Never,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceWatchPolicy {
    #[default]
    Disabled,
    Ensure,
    Enabled,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Foreground,
    Background,
    Wait,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Auto,
    Summary,
    Full,
    Inline,
    Artifact,
    Path,
    JobOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactMode {
    None,
    OnLargeOutput,
    Always,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePhase {
    Queued,
    Requested,
    Resolving,
    Routing,
    Authorizing,
    Planning,
    Leasing,
    Discovering,
    Diffing,
    Fetching,
    Rendering,
    Enriching,
    Normalizing,
    Parsing,
    Graphing,
    Preparing,
    Batching,
    Embedding,
    Vectorizing,
    Upserting,
    Retrieving,
    Synthesizing,
    Evaluating,
    Publishing,
    Cleaning,
    Complete,
    Canceled,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StageApplicability {
    #[default]
    Always,
    WhenChanged,
    WhenEmbedding,
    WhenScopeMatches,
    Optional,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StageSkipReason {
    NotApplicable,
    Disabled,
    NoChanges,
    Reused,
    EmptyInput,
    Unsupported,
    Canceled,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Source,
    Watch,
    Map,
    Extract,
    Research,
    Ask,
    Query,
    Retrieve,
    Memory,
    Graph,
    Prune,
    ProviderProbe,
    Reset,
}

impl JobKind {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Source,
            Self::Watch,
            Self::Map,
            Self::Extract,
            Self::Research,
            Self::Ask,
            Self::Query,
            Self::Retrieve,
            Self::Memory,
            Self::Graph,
            Self::Prune,
            Self::ProviderProbe,
            Self::Reset,
        ]
    }

    /// Public Source-pipeline job kinds advertised in REST/MCP/generated schemas.
    pub const fn is_public_source_surface(self) -> bool {
        true
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobIntent {
    #[default]
    Run,
    Acquire,
    Refresh,
    Watch,
    Exec,
    Retry,
    Recover,
    Cleanup,
    Probe,
    Reset,
    /// Initial source indexing submission.
    Index,
    /// Discover items without embedding (`JobKind::Map`).
    Map,
    /// Structured LLM extraction (`JobKind::Extract`).
    Extract,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobRetryMode {
    SameConfig,
    WithOverrides,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    WebPage,
    RepoFile,
    LocalFile,
    PackageVersion,
    FeedEntry,
    Transcript,
    SessionTurn,
    ToolCall,
    CliOutput,
    McpToolOutput,
    MemoryRecord,
    Artifact,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Code,
    Markdown,
    Html,
    PlainText,
    Transcript,
    Structured,
    Json,
    Yaml,
    Toml,
    Xml,
    BinaryMetadata,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Queued,
    Pending,
    Running,
    Waiting,
    Blocked,
    Canceling,
    Completed,
    CompletedDegraded,
    Failed,
    Canceled,
    Expired,
    Skipped,
}

include!("enums/runtime.rs");
