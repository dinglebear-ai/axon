mod config;
mod config_impls;
mod enums;
pub mod overrides;
pub mod subconfigs;

pub use config::Config;
pub use enums::{
    ClientMode, CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, PerformanceProfile,
    RedditSort, RedditTime, RenderMode, ScrapeFormat,
};
pub use overrides::ConfigOverrides;

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
