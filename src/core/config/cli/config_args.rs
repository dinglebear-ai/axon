use clap::{ArgAction, Args, Subcommand};

#[derive(Debug, Args)]
pub(in crate::core::config) struct SyncArgs {
    #[command(subcommand)]
    pub(in crate::core::config) action: Option<SyncSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(in crate::core::config) enum SyncSubcommand {
    /// Show local artifacts waiting to be reconciled with the server
    Pending,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(in crate::core::config) struct ConfigArgs {
    #[command(subcommand)]
    pub(in crate::core::config) action: Option<ConfigSubcommand>,
}

#[derive(Debug, Subcommand)]
pub(in crate::core::config) enum ConfigSubcommand {
    /// List every entry from .env and config.toml (secrets redacted)
    List {
        /// Restrict listing to .env entries
        #[arg(long, action = ArgAction::SetTrue)]
        env: bool,
        /// Restrict listing to config.toml entries
        #[arg(long, action = ArgAction::SetTrue)]
        toml: bool,
        /// Reveal secret values instead of showing `***`
        #[arg(long, action = ArgAction::SetTrue)]
        reveal: bool,
    },
    /// Print a single value (auto-detects file by key shape)
    Get {
        /// UPPER_SNAKE for .env, dotted lowercase for config.toml
        key: String,
        /// Force read from .env regardless of key shape
        #[arg(long, action = ArgAction::SetTrue)]
        env: bool,
        /// Force read from config.toml regardless of key shape
        #[arg(long, action = ArgAction::SetTrue)]
        toml: bool,
        /// Reveal secret values instead of showing `***`
        #[arg(long, action = ArgAction::SetTrue)]
        reveal: bool,
    },
    /// Write a value. Auto-detects file: UPPER_SNAKE to .env, dotted lowercase to config.toml
    Set {
        key: String,
        value: String,
        /// Force write to .env regardless of key shape
        #[arg(long, action = ArgAction::SetTrue)]
        env: bool,
        /// Force write to config.toml regardless of key shape
        #[arg(long, action = ArgAction::SetTrue)]
        toml: bool,
    },
    /// Remove a value from .env or config.toml
    Unset {
        key: String,
        #[arg(long, action = ArgAction::SetTrue)]
        env: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        toml: bool,
    },
    /// Print resolved paths to .env and config.toml
    Path,
    /// Manage saved LLM provider/model profiles ([providers.<name>] in config.toml)
    Provider {
        #[command(subcommand)]
        action: Option<ProviderSubcommand>,
    },
}

#[derive(Debug, Subcommand)]
pub(in crate::core::config) enum ProviderSubcommand {
    /// List saved profiles and the effective active backend
    List,
    /// Show one profile's fields (api-key redacted unless --reveal)
    Show {
        name: String,
        /// Reveal secret values instead of showing `***`
        #[arg(long, action = ArgAction::SetTrue)]
        reveal: bool,
    },
    /// Activate a saved profile (sets [llm] active-provider)
    Use { name: String },
    /// Create/replace a profile: add <name> <backend> [field=value ...]
    Add {
        name: String,
        /// gemini-headless | openai-compat | codex-app-server
        backend: String,
        /// Extra fields as field=value (model, base-url, api-key, cmd, home)
        #[arg(value_name = "FIELD=VALUE")]
        fields: Vec<String>,
    },
    /// Set one field on a profile: set <name> <field> <value>
    Set {
        name: String,
        field: String,
        value: String,
    },
    /// Delete a profile (clears it as active if it was)
    Remove { name: String },
}
