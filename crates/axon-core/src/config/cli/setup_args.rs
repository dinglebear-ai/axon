use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args, Default)]
pub(in crate::config) struct SetupInitArgs {
    /// MCP HTTP bind host written to AXON_MCP_HTTP_HOST
    #[arg(long = "mcp-host")]
    pub(in crate::config) mcp_host: Option<String>,
    /// MCP HTTP bind port written to AXON_MCP_HTTP_PORT
    #[arg(long = "mcp-port")]
    pub(in crate::config) mcp_port: Option<u16>,
    /// MCP auth mode. Bearer generates/requires AXON_MCP_HTTP_TOKEN; OAuth requires Google OAuth vars.
    #[arg(long = "auth-mode", value_enum)]
    pub(in crate::config) auth_mode: Option<SetupAuthMode>,
    /// Static bearer token for AXON_MCP_HTTP_TOKEN
    #[arg(long = "mcp-token")]
    pub(in crate::config) mcp_token: Option<String>,
    /// Public URL for OAuth metadata and callbacks
    #[arg(long = "oauth-public-url")]
    pub(in crate::config) oauth_public_url: Option<String>,
    /// Google OAuth client ID
    #[arg(long = "google-client-id")]
    pub(in crate::config) google_client_id: Option<String>,
    /// Google OAuth client secret
    #[arg(long = "google-client-secret")]
    pub(in crate::config) google_client_secret: Option<String>,
    /// Admin email allowed to complete OAuth auth
    #[arg(long = "auth-admin-email")]
    pub(in crate::config) auth_admin_email: Option<String>,
    /// Tavily API key for search/research
    #[arg(long = "tavily-api-key")]
    pub(in crate::config) tavily_api_key: Option<String>,
    /// GitHub token for higher-rate GitHub ingest
    #[arg(long = "github-token")]
    pub(in crate::config) github_token: Option<String>,
    /// Reddit client ID for Reddit ingest
    #[arg(long = "reddit-client-id")]
    pub(in crate::config) reddit_client_id: Option<String>,
    /// Reddit client secret for Reddit ingest
    #[arg(long = "reddit-client-secret")]
    pub(in crate::config) reddit_client_secret: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(in crate::config) enum SetupAuthMode {
    Bearer,
    Oauth,
}

#[derive(Debug, Args)]
pub(in crate::config) struct ComposeArgs {
    #[command(subcommand)]
    pub(in crate::config) action: ComposeSubcommand,
}

#[derive(Debug, Subcommand)]
pub(in crate::config) enum ComposeSubcommand {
    /// Pull and start the Docker service stack
    Up,
    /// Stop the Docker service stack
    Down,
    /// Restart running services
    Restart,
    /// Rebuild the Axon image and start the stack
    Rebuild,
}
