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
use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, parser::ValueSource};

pub use docker::is_docker_service_host;

pub fn validate_toml_config_text(raw_toml: &str) -> Result<(), String> {
    toml::from_str::<toml_config::raw::RawTomlConfig>(raw_toml)
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
    let matches = command.clone().get_matches_from(routed_args);
    if let Err(message) = validate_relevant_globals(&command, &matches) {
        report_error(&message);
        std::process::exit(2);
    }
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

fn validate_relevant_globals(command: &Command, matches: &ArgMatches) -> Result<(), String> {
    let path = command_path(matches);
    let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
    let relevant = super::help::relevant_global_ids(&path_refs);
    let local_ids = local_argument_ids(command, &path);
    for arg in command.get_arguments().filter(|arg| arg.is_global_set()) {
        let id = arg.get_id().as_str();
        if value_source_recursive(matches, id) == Some(ValueSource::CommandLine)
            && !relevant.contains(id)
            && !local_ids.contains(id)
        {
            let flag = arg
                .get_long()
                .map(|long| format!("--{long}"))
                .unwrap_or_else(|| id.replace('_', "-"));
            if id == "watch" {
                return Err("--watch is only supported with `axon status`".to_string());
            }
            let command_name = if path.is_empty() {
                "this command".to_string()
            } else {
                format!("`axon {}`", path.join(" "))
            };
            let help_command = if path.is_empty() {
                "axon --help".to_string()
            } else {
                format!("axon {} --help", path.join(" "))
            };
            return Err(format!(
                "{flag} is not supported by {command_name}; run `{help_command}` to see valid options"
            ));
        }
    }
    Ok(())
}

fn local_argument_ids<'a>(
    command: &'a Command,
    path: &[String],
) -> std::collections::HashSet<&'a str> {
    let mut current = command;
    for segment in path {
        let Some(next) = current.find_subcommand(segment) else {
            break;
        };
        current = next;
    }
    current
        .get_arguments()
        .filter(|arg| !arg.is_global_set())
        .map(|arg| arg.get_id().as_str())
        .collect()
}

fn command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = matches;
    while let Some((name, subcommand)) = current.subcommand() {
        path.push(name.to_string());
        current = subcommand;
    }
    path
}

fn value_source_recursive(matches: &ArgMatches, id: &str) -> Option<ValueSource> {
    matches.value_source(id).or_else(|| {
        matches
            .subcommand()
            .and_then(|(_, subcommand)| value_source_recursive(subcommand, id))
    })
}
#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
