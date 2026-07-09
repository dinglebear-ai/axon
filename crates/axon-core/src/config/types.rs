mod config;
mod config_debug;
mod config_impls;
mod enums;
pub mod freshness;
pub mod overrides;
mod session_watch;
pub mod subconfigs;

pub const DEFAULT_CRAWL_BROADCAST_BUFFER_MIN: usize = 512;
pub const DEFAULT_CRAWL_BROADCAST_BUFFER_MAX: usize = 2_048;
pub const DEFAULT_MAX_PAGE_BYTES: u64 = 4 * 1024 * 1024;
pub const DEFAULT_CRAWL_MEMORY_ABORT_PERCENT: f64 = 85.0;

pub use config::{AdaptiveConcurrencyConfig, Config};
pub use enums::{
    ColorChoice, CommandKind, ConfigValueSource, EvaluateResponsesMode, MapFallback, McpTransport,
    PerformanceProfile, RedditSort, RedditTime, RenderMode, ScrapeFormat,
};
pub use freshness::{FreshAction, FreshDuration, FreshnessCommand, FreshnessRequest};
pub use overrides::ConfigOverrides;
pub use session_watch::{
    CodeSearchWatchConfig, SessionWatchConfig, SessionWatchServiceAction, SessionsRuntimeAction,
};
#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
