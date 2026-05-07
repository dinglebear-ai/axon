mod cli;
mod help;
pub(crate) mod parse;
pub mod secret;
mod types;
pub mod validation;

pub use parse::{build_cli_command, parse_args};
pub use secret::Secret;
pub use types::{
    CommandKind, Config, ConfigOverrides, EvaluateResponsesMode, MapFallback, McpTransport,
    PerformanceProfile, RedditSort, RedditTime, RenderMode, ScrapeFormat,
};
pub use validation::{CollectionNameError, validate_collection_name};
