pub(crate) mod build_config;
pub mod docker;
pub mod env_registry;
pub(crate) mod excludes;
pub(crate) mod helpers;
mod performance;
mod toml_config;
pub mod tuning;

use super::cli::Cli;
use super::help::maybe_print_top_level_help_and_exit;
use super::types::Config;
use crate::ui::report_error;
use clap::{Command, CommandFactory, FromArgMatches, parser::ValueSource};

pub use docker::is_docker_service_host;

pub fn validate_toml_config_text(raw_toml: &str) -> Result<(), String> {
    toml::from_str::<toml_config::TomlConfig>(raw_toml)
        .map(|_| ())
        .map_err(|e| format!("config TOML parse error: {e}"))
}

pub fn build_cli_command() -> Command {
    Cli::command()
}

pub fn parse_args() -> Config {
    maybe_print_top_level_help_and_exit();
    // Route a bare leading source token (`axon https://x`, `axon ./dir`,
    // `axon r/rust`, `axon pkg:npm/foo`) through the `source` subcommand before
    // clap parses. Explicit subcommands and `axon source <x>` are untouched.
    let command = Cli::command();
    let routed_args =
        super::source_routing::route_bare_source(std::env::args().collect(), &command);
    let matches = command.get_matches_from(routed_args);
    let output_dir_was_explicit =
        matches.value_source("output_dir") == Some(ValueSource::CommandLine);
    let collection_was_explicit =
        matches.value_source("collection") == Some(ValueSource::CommandLine);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    match build_config::into_config_with_sources(
        cli,
        output_dir_was_explicit,
        collection_was_explicit,
    ) {
        Ok(cfg) => cfg,
        Err(msg) => {
            report_error(&msg);
            std::process::exit(1);
        }
    }
}
#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
