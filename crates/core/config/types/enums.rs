use clap::ValueEnum;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum CommandKind {
    Scrape,
    Crawl,
    Refresh,
    Watch,
    Map,
    Extract,
    Search,
    Embed,
    Debug,
    Doctor,
    Query,
    Retrieve,
    Ask,
    Evaluate,
    Suggest,
    Sources,
    Domains,
    Stats,
    Status,
    Dedupe,
    Ingest,
    Sessions,
    Research,
    Screenshot,
    Graph,
    Completions,
    Mcp,
    Serve,
    Migrate,
    Export,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scrape => "scrape",
            Self::Crawl => "crawl",
            Self::Refresh => "refresh",
            Self::Watch => "watch",
            Self::Map => "map",
            Self::Extract => "extract",
            Self::Search => "search",
            Self::Embed => "embed",
            Self::Debug => "debug",
            Self::Doctor => "doctor",
            Self::Query => "query",
            Self::Retrieve => "retrieve",
            Self::Ask => "ask",
            Self::Evaluate => "evaluate",
            Self::Suggest => "suggest",
            Self::Sources => "sources",
            Self::Domains => "domains",
            Self::Stats => "stats",
            Self::Status => "status",
            Self::Dedupe => "dedupe",
            Self::Ingest => "ingest",
            Self::Sessions => "sessions",
            Self::Research => "research",
            Self::Screenshot => "screenshot",
            Self::Graph => "graph",
            Self::Completions => "completions",
            Self::Mcp => "mcp",
            Self::Serve => "serve",
            Self::Migrate => "migrate",
            Self::Export => "export",
        }
    }
}

impl fmt::Display for CommandKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderMode {
    Http,
    Chrome,
    #[value(name = "auto-switch")]
    AutoSwitch,
}

impl fmt::Display for RenderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Http => "http",
            Self::Chrome => "chrome",
            Self::AutoSwitch => "auto-switch",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScrapeFormat {
    Markdown,
    Html,
    #[value(name = "rawHtml")]
    #[serde(rename = "rawHtml")]
    RawHtml,
    Json,
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
        let value = match self {
            Self::Hot => "hot",
            Self::Top => "top",
            Self::New => "new",
            Self::Rising => "rising",
        };
        f.write_str(value)
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
        let value = match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
            Self::All => "all",
        };
        f.write_str(value)
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
        let value = match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Both => "both",
        };
        f.write_str(value)
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
        let value = match self {
            Self::Inline => "inline",
            Self::SideBySide => "side-by-side",
            Self::Events => "events",
        };
        f.write_str(value)
    }
}
