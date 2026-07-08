use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReservationStatus {
    Requested,
    Queued,
    Granted,
    Active,
    Released,
    Expired,
    Canceled,
    Failed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PublishState {
    Planning,
    Writing,
    Publishing,
    Committed,
    CleanupPending,
    Cleaning,
    Cleaned,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DocumentLifecycleStatus {
    Discovered,
    Fetched,
    Normalized,
    Enriched,
    Parsed,
    Prepared,
    Embedded,
    Vectorized,
    Published,
    Cleaned,
    Degraded,
    Failed,
    Skipped,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Added,
    Modified,
    Removed,
    Unchanged,
    Skipped,
    Failed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentKind {
    None,
    Metadata,
    Classification,
    Summary,
    Extraction,
    Authority,
    Dependency,
    ApiSchema,
    ToolSchema,
    Session,
    Custom,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentStatus {
    NotNeeded,
    Pending,
    Completed,
    Degraded,
    Failed,
    Skipped,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CleanupDebtKind {
    VectorDelete,
    ArtifactDelete,
    LedgerPrune,
    GraphPrune,
    MemoryPrune,
    JobRetention,
    CachePrune,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Llm,
    Embedding,
    Vector,
    Search,
    Fetch,
    Render,
    NetworkCapture,
    Artifact,
    Ledger,
    Graph,
    Memory,
    Job,
    Watch,
    Config,
    Credential,
    Cache,
    Security,
    RateLimiter,
    HealthProbe,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unavailable,
    Cooling,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Internal,
    Sensitive,
    Redacted,
    Derived,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Degraded,
    Failed,
    Fatal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    Interactive,
    High,
    Normal,
    Background,
    Maintenance,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityLevel {
    Official,
    Verified,
    UserPinned,
    Inferred,
    Community,
    Mirror,
    Conflicting,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAffinity {
    Inline,
    Worker,
    Scheduler,
    ProviderBound,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SafetyClass {
    PublicNetwork,
    AuthenticatedNetwork,
    LocalFilesystem,
    ToolExecution,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    ApiKey,
    OAuthToken,
    BearerToken,
    BasicAuth,
    Cookie,
    SshKey,
    LocalConfig,
}
