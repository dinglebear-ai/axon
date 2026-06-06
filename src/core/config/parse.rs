mod build_config;
pub(crate) mod docker;
pub(crate) mod env_registry;
pub(crate) mod excludes;
pub(crate) mod helpers;
mod performance;
mod toml_config;
pub(crate) mod tuning;

use super::cli::Cli;
use super::help::maybe_print_top_level_help_and_exit;
use super::types::Config;
use crate::core::ui::report_error;
use clap::{Command, CommandFactory, FromArgMatches, parser::ValueSource};

pub(crate) use docker::is_docker_service_host;

pub(crate) fn validate_toml_config_text(raw_toml: &str) -> Result<(), String> {
    toml::from_str::<toml_config::TomlConfig>(raw_toml)
        .map(|_| ())
        .map_err(|e| format!("config TOML parse error: {e}"))
}

/// The effective LLM selection resolved from the persisted config + env, for
/// display/admin callers such as `axon config provider list`. This reuses the
/// same provider-overlay resolution the real config path uses, so the listing
/// cannot drift from what an actual `ask` would run. `backend` carries the
/// resolution error (e.g. a broken active profile) so callers can surface it
/// inline instead of printing a misleading default.
pub(crate) struct EffectiveLlm {
    pub active_provider: Option<String>,
    pub backend: Result<crate::services::llm_backend::LlmBackendKind, String>,
}

/// Resolve [`EffectiveLlm`] from the persisted config.toml + env. `provider_flag`
/// is the `--provider` override; pass `None` to reflect only the persisted
/// selection (`AXON_PROVIDER` env > `[llm] active-provider`). Errors only when
/// the config file itself is malformed; a broken *active profile* is reported via
/// `EffectiveLlm::backend` (an `Err`), not this outer `Result`.
pub(crate) fn effective_llm(provider_flag: Option<&str>) -> Result<EffectiveLlm, String> {
    let toml = toml_config::load_toml_config()?;
    Ok(EffectiveLlm {
        active_provider: build_config::provider_overlay::active_provider_name(&toml, provider_flag),
        backend: build_config::provider_overlay::effective_backend_kind(&toml, provider_flag),
    })
}

pub fn build_cli_command() -> Command {
    Cli::command()
}

pub fn parse_args() -> Config {
    maybe_print_top_level_help_and_exit();
    let matches = Cli::command().get_matches();
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
