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
    Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    #[default]
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
