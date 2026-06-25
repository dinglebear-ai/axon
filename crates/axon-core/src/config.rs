mod cli;
mod help;
pub mod parse;
pub mod secret;
mod types;
pub mod validation;

pub use parse::{build_cli_command, parse_args};
pub use secret::Secret;
pub use types::{
    AdaptiveConcurrencyConfig, ColorChoice, CommandKind, Config, ConfigOverrides,
    EvaluateResponsesMode, MapFallback, McpTransport, PerformanceProfile, RedditSort, RedditTime,
    RenderMode, ScrapeFormat, SessionWatchConfig, SessionWatchServiceAction, SessionsRuntimeAction,
};
pub use validation::{CollectionNameError, validate_collection_name};
