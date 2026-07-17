use clap::{ArgAction, Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub(crate) enum ResourceCliCommand {
    /// List, inspect, or read artifacts by opaque artifact id
    Artifacts(ArtifactsArgs),
    /// Stage local files as durable uploads
    Uploads(UploadsArgs),
    /// Inspect configured vector collections
    Collections(CollectionsArgs),
    /// Query the read-only SourceGraph
    Graph(GraphArgs),
    /// Inspect provider capabilities and health
    Providers(ProvidersArgs),
    /// Print machine-readable runtime capabilities
    Capabilities,
    /// Send a direct prompt to the configured LLM
    Chat(ChatArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ArtifactsArgs {
    #[command(subcommand)]
    pub(crate) action: ArtifactSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ArtifactSubcommand {
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "source-id")]
        source_id: Option<String>,
        #[arg(long = "job-id")]
        job_id: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    Get {
        artifact_id: String,
        #[arg(long = "include-content-url", action = ArgAction::SetTrue)]
        include_content_url: bool,
    },
    Content {
        artifact_id: String,
        #[arg(long, action = ArgAction::SetTrue)]
        download: bool,
        #[arg(long)]
        range: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct UploadsArgs {
    #[command(subcommand)]
    pub(crate) action: UploadSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UploadSubcommand {
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    Get {
        upload_id: String,
    },
    Create {
        path: PathBuf,
        #[arg(long, default_value = "source_artifact")]
        purpose: String,
        #[arg(long = "source-hint")]
        source_hint: Option<String>,
    },
    Complete {
        upload_id: String,
        #[arg(long)]
        sha256: Option<String>,
        #[arg(long = "source-option", value_name = "KEY=VALUE")]
        source_options: Vec<String>,
    },
    Abort {
        upload_id: String,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct CollectionsArgs {
    #[command(subcommand)]
    pub(crate) action: CollectionSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CollectionSubcommand {
    List,
    Get {
        collection: String,
        #[arg(long = "include-schema", action = ArgAction::SetTrue)]
        include_schema: bool,
        #[arg(long = "include-indexes", action = ArgAction::SetTrue)]
        include_indexes: bool,
    },
}

#[derive(Debug, Args)]
pub(crate) struct GraphArgs {
    #[command(subcommand)]
    pub(crate) action: GraphSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GraphSubcommand {
    Kinds,
    Resolve {
        identifier: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    Query {
        query: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    Node {
        node_id: String,
        #[arg(long = "include-edges", action = ArgAction::SetTrue)]
        include_edges: bool,
        #[arg(long = "include-evidence", action = ArgAction::SetTrue)]
        include_evidence: bool,
    },
    Edge {
        edge_id: String,
        #[arg(long = "include-evidence", action = ArgAction::SetTrue)]
        include_evidence: bool,
    },
    Source {
        source_id: String,
        #[arg(long)]
        depth: Option<usize>,
        #[arg(long = "edge-kind")]
        edge_kind: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ProvidersArgs {
    #[command(subcommand)]
    pub(crate) action: ProviderSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProviderSubcommand {
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    Get {
        provider: String,
        #[arg(long = "include-health", action = ArgAction::SetTrue)]
        include_health: bool,
        #[arg(long = "include-limits", action = ArgAction::SetTrue)]
        include_limits: bool,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ChatArgs {
    #[arg(value_name = "MESSAGE", required = true)]
    pub(crate) message: Vec<String>,
}
