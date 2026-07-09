use clap::ValueEnum;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Watch,
    Monitor,
    Map,
    Endpoints,
    Extract,
    Search,
    Brand,
    Debug,
    Diff,
    Doctor,
    Query,
    Retrieve,
    Ask,
    Summarize,
    Evaluate,
    Train,
    Suggest,
    Sources,
    Domains,
    Stats,
    Status,
    Jobs,
    Refresh,
    Fresh,
    Memory,
    Sessions,
    Source,
    Research,
    Screenshot,
    Completions,
    Mcp,
    Serve,
    Reset,
    Prune,
    Preflight,
    Smoke,
    Compose,
    Setup,
    Migrate,
    Config,
    Sync,
    Update,
    Palette,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::Monitor => "monitor",
            Self::Map => "map",
            Self::Endpoints => "endpoints",
            Self::Extract => "extract",
            Self::Search => "search",
            Self::Brand => "brand",
            Self::Debug => "debug",
            Self::Diff => "diff",
            Self::Doctor => "doctor",
            Self::Query => "query",
            Self::Retrieve => "retrieve",
            Self::Ask => "ask",
            Self::Summarize => "summarize",
            Self::Evaluate => "evaluate",
            Self::Train => "train",
            Self::Suggest => "suggest",
            Self::Sources => "sources",
            Self::Domains => "domains",
            Self::Stats => "stats",
            Self::Status => "status",
            Self::Jobs => "jobs",
            Self::Refresh => "refresh",
            Self::Fresh => "fresh",
            Self::Memory => "memory",
            Self::Sessions => "sessions",
            Self::Source => "source",
            Self::Research => "research",
            Self::Screenshot => "screenshot",
            Self::Completions => "completions",
            Self::Mcp => "mcp",
            Self::Serve => "serve",
            Self::Reset => "reset",
            Self::Prune => "prune",
            Self::Preflight => "preflight",
            Self::Smoke => "smoke",
            Self::Compose => "compose",
            Self::Setup => "setup",
            Self::Migrate => "migrate",
            Self::Config => "config",
            Self::Sync => "sync",
            Self::Update => "update",
            Self::Palette => "palette",
        }
    }
}

impl fmt::Display for CommandKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    ValueEnum,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RenderMode {
    Http,
    Chrome,
    #[value(name = "auto-switch")]
    #[serde(alias = "auto", alias = "autoswitch")]
    AutoSwitch,
}

impl fmt::Display for RenderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Http => "http",
            Self::Chrome => "chrome",
            Self::AutoSwitch => "auto-switch",
        })
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    ValueEnum,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ScrapeFormat {
    Markdown,
    Html,
    #[value(name = "rawHtml")]
    #[serde(rename = "rawHtml")]
    RawHtml,
    Json,
    Llm,
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RedditSort {
    Hot,
    Top,
    New,
    Rising,
}

impl fmt::Display for RedditSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hot => "hot",
            Self::Top => "top",
            Self::New => "new",
            Self::Rising => "rising",
        })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RedditTime {
    Hour,
    Day,
    Week,
    Month,
    Year,
    All,
}

impl fmt::Display for RedditTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
            Self::All => "all",
        })
    }
}

/// Fallback strategy when `axon map` finds no sitemap documents.
///
/// `Structure` (default): fetch the root page and extract anchor hrefs (bounded, fast).
/// `Crawl`: run a full Spider.rs crawl (slow, legacy behaviour — explicit opt-in only).
#[derive(
    Debug, Clone, Copy, Default, ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "kebab-case")]
pub enum MapFallback {
    #[default]
    Structure,
    Crawl,
}

impl fmt::Display for MapFallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Structure => "structure",
            Self::Crawl => "crawl",
        })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerformanceProfile {
    #[value(name = "high-stable")]
    HighStable,
    Extreme,
    Balanced,
    Max,
}

#[derive(
    Debug, Clone, Copy, Default, ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    Stdio,
    #[default]
    Http,
    Both,
}

impl fmt::Display for McpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Both => "both",
        })
    }
}

/// Terminal color override. Wired through `Config::color_choice` to both
/// `core::ui::color_enabled()` and `core::logging::should_use_ansi()` so the
/// runtime override is single-source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ColorChoice {
    /// Detect TTY + honor NO_COLOR / FORCE_COLOR / CLICOLOR_FORCE (default).
    #[default]
    Auto,
    /// Force ANSI color output regardless of TTY detection.
    Always,
    /// Suppress all ANSI escapes.
    Never,
}

impl fmt::Display for ColorChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluateResponsesMode {
    Inline,
    #[value(name = "side-by-side")]
    SideBySide,
    Events,
}

impl fmt::Display for EvaluateResponsesMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inline => "inline",
            Self::SideBySide => "side-by-side",
            Self::Events => "events",
        })
    }
}

/// Where a config value was actually sourced from. Used to make a value's
/// "distinct explicit confirmation" guarantee provable rather than assumed:
/// `reset_confirm_legacy_wipe` (see `crates/axon-services/src/reset.rs`) must
/// only ever be honored when this is `CliFlag` — never `TomlFile`/`EnvVar`,
/// which could be set once in a persisted file/environment and silently
/// defeat the per-invocation confirmation the flag is meant to provide.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfigValueSource {
    #[default]
    Unset,
    CliFlag,
    TomlFile,
    EnvVar,
}
