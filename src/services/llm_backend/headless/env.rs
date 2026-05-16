use tokio::process::Command;

#[cfg(test)]
use std::ffi::OsString;

const ALLOWED_ENV_KEYS: &[&str] = &[
    "HOME",
    "LANG",
    "LC_ALL",
    "PATH",
    "TERM",
    "TZ",
    "USER",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_API_KEY",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GEMINI_API_KEY",
];

pub fn apply_env_allowlist(command: &mut Command) {
    command.env_clear();
    for key in ALLOWED_ENV_KEYS {
        if let Some(value) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            command.env(key, value);
        }
    }
}

#[cfg(test)]
pub fn allowed_env_keys() -> &'static [&'static str] {
    ALLOWED_ENV_KEYS
}

#[cfg(test)]
fn capture_allowed_env(input: &[(&str, &str)]) -> Vec<(&'static str, OsString)> {
    ALLOWED_ENV_KEYS
        .iter()
        .filter_map(|allowed| {
            input
                .iter()
                .find(|(key, _)| key == allowed)
                .map(|(_, value)| (*allowed, OsString::from(value)))
        })
        .collect()
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
