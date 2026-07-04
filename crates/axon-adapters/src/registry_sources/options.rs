//! Adapter option validation for the registry source adapter.

use std::path::PathBuf;

use axon_api::source::*;

use crate::adapter::Result;

const ALLOWED_OPTIONS: &[&str] = &["registry_dump_path", "include_all_versions"];

#[derive(Debug, Clone)]
pub(crate) struct RegistryOptions {
    pub(crate) dump_path: PathBuf,
    /// When true, every version in the dump becomes a manifest item.
    /// When false (default), only the latest version is discovered.
    pub(crate) include_all_versions: bool,
}

pub(crate) fn validate_options(options: &AdapterOptions) -> Result<RegistryOptions> {
    for key in options.values.keys() {
        if !ALLOWED_OPTIONS.contains(&key.as_str()) {
            return Err(option_invalid(
                key,
                "registry adapter option is not supported",
            ));
        }
    }
    let dump_path = options
        .values
        .get("registry_dump_path")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| {
            option_invalid(
                "registry_dump_path",
                "registry adapter requires registry_dump_path option",
            )
        })?;
    let include_all_versions = match options.values.get("include_all_versions") {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| option_invalid("include_all_versions", "expected a boolean"))?,
        None => false,
    };
    Ok(RegistryOptions {
        dump_path,
        include_all_versions,
    })
}

fn option_invalid(key: &str, message: &str) -> ApiError {
    ApiError::new(
        "adapter.registry.option.invalid",
        axon_error::ErrorStage::Routing,
        message,
    )
    .with_context("option", key.to_string())
}

#[cfg(test)]
#[path = "../registry_sources_options_tests.rs"]
mod tests;
