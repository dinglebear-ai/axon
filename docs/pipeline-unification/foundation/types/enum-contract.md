# Enum Contract
Last Modified: 2026-06-30

## Contract

Enums are closed, documented discriminants owned by `axon-api`. They define
pipeline state, source kind, scope, content kind, provider kind, visibility,
severity, and lifecycle behavior.

This file is the sole source of truth for serialized enum values. Every enum
value used by adapter scopes, metadata payloads, graph contracts,
observability/events, errors, REST, MCP, CLI schemas, and generated docs must
appear here. Contract checks must fail when another doc introduces a stable enum
string that is absent from this file.

## Rules

- Rust variants are PascalCase.
- JSON values are snake_case.
- Unknown external enum values fail request validation unless the enum is
  explicitly marked forward-compatible.
- Do not use raw strings for stable pipeline phases, statuses, source kinds, or
  provider kinds.
- Adding a variant requires contract update, schema update, and tests.

## Required Enums

```rust
pub enum SourceIntent { Acquire, Refresh, Watch, Map }
pub enum SourceRefreshPolicy { IfStale, Force, Never }
pub enum SourceWatchPolicy { Disabled, Ensure, Enabled }
pub enum ExecutionMode { Foreground, Background, Wait }
pub enum ResponseMode { Auto, Summary, Full, Inline, Artifact, Path, JobOnly }
pub enum ArtifactMode { None, OnLargeOutput, Always }

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

/// Owned by `axon-error` (`ErrorStage`). More specific than `PipelinePhase`:
/// every direct-projection `ErrorStage` shares a name with a `PipelinePhase`
/// value above; the remaining values (`ParsingContent`, `Observing`,
/// `Storage`, `Provider`, `Transport`, `Internal`) are error-boundary-only and
/// must not be added to `PipelinePhase`. See `schemas/error-schema.md`
/// "Error Stage to Event Phase Projection" for the projection rules.
pub enum ErrorStage {
    Parsing,
    Validation,
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
    ParsingContent,
    Graphing,
    Preparing,
    Batching,
    Embedding,
    Vectorizing,
    Upserting,
    Publishing,
    Cleaning,
    Retrieving,
    Synthesizing,
    Evaluating,
    Observing,
    Storage,
    Provider,
    Transport,
    Internal,
}

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

pub enum DiffKind { Added, Modified, Removed, Unchanged, Skipped, Failed }

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

pub enum EnrichmentStatus {
    NotNeeded,
    Pending,
    Completed,
    Degraded,
    Failed,
    Skipped,
}

pub enum CleanupDebtKind {
    VectorDelete,
    ArtifactDelete,
    LedgerPrune,
    GraphPrune,
    MemoryPrune,
    JobRetention,
    CachePrune,
}

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

pub enum HealthStatus { Healthy, Degraded, Unavailable, Cooling, Unknown }
pub enum Visibility { Public, Internal, Sensitive, Redacted, Derived }
pub enum Severity { Debug, Info, Warning, Degraded, Failed, Fatal }
pub enum JobPriority { Interactive, High, Normal, Background, Maintenance }
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
pub enum ExecutionAffinity { Inline, Worker, Scheduler, ProviderBound }
pub enum SafetyClass { PublicNetwork, AuthenticatedNetwork, LocalFilesystem, ToolExecution }
pub enum CredentialKind { ApiKey, OAuthToken, BearerToken, BasicAuth, Cookie, SshKey, LocalConfig }
pub enum ArtifactKind { RawContent, NormalizedContent, Manifest, Report, Screenshot, Warc, ProviderTrace }
pub enum CachePolicy { Bypass, Use, Revalidate, Offline }
pub enum PayloadFieldSchema { Keyword, Integer, Float, Boolean, Datetime, Text }
pub enum ChunkProfile { CodeSymbol, CodeManifest, MarkdownSections, HtmlArticle, PlainTextWindows, TranscriptSegments, StructuredRecords, ApiSchema, ToolOutput, SessionTurns, AtomicMetadata }
pub enum TransportKind { Cli, Rest, Mcp, Watch, Worker, System }
```

## JSON Naming Examples

| Rust | JSON |
|---|---|
| `SourceKind::CliTool` | `"cli_tool"` |
| `SourceScope::Subreddit` | `"subreddit"` |
| `ResponseMode::Auto` | `"auto"` |
| `LifecycleStatus::CompletedDegraded` | `"completed_degraded"` |
| `ProviderKind::NetworkCapture` | `"network_capture"` |

## Completion Checklist

- every enum has JSON serialization tests
- every enum appears in generated JSON schema when externally exposed
- every enum has docs for when each variant is used
- no stable status/kind/scope is represented as an untyped string
- schema generation checks fail when docs or registries use stable enum values
  not listed here
