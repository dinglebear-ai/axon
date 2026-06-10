//! Claude Code plugin-option → `AXON_*` env-var mapping.
//!
//! This is the Rust port of the env-var mapping that used to live in the bash
//! `plugin-setup.sh` SessionStart hook. It runs before `parse_args()` builds the
//! `Config` (which reads the `AXON_*` env vars), preserving the old hook's
//! pre-Config timing where the bash script `export`ed these before exec'ing
//! `axon`.

/// Mapping of `CLAUDE_PLUGIN_OPTION_*` env vars (set by Claude Code from the
/// plugin's `userConfig`) to the `AXON_*` env vars the rest of the binary reads.
/// Each pair is `(option_env, axon_env)`.
const PLUGIN_OPTION_MAPPINGS: &[(&str, &str)] = &[
    ("CLAUDE_PLUGIN_OPTION_API_TOKEN", "AXON_MCP_HTTP_TOKEN"),
    ("CLAUDE_PLUGIN_OPTION_TAVILY_API_KEY", "TAVILY_API_KEY"),
    ("CLAUDE_PLUGIN_OPTION_GITHUB_TOKEN", "GITHUB_TOKEN"),
    ("CLAUDE_PLUGIN_OPTION_REDDIT_CLIENT_ID", "REDDIT_CLIENT_ID"),
    (
        "CLAUDE_PLUGIN_OPTION_REDDIT_CLIENT_SECRET",
        "REDDIT_CLIENT_SECRET",
    ),
    ("CLAUDE_PLUGIN_OPTION_AUTH_MODE", "AXON_MCP_AUTH_MODE"),
    ("CLAUDE_PLUGIN_OPTION_PUBLIC_URL", "AXON_MCP_PUBLIC_URL"),
    (
        "CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID",
        "AXON_MCP_GOOGLE_CLIENT_ID",
    ),
    (
        "CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET",
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
    ),
    (
        "CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL",
        "AXON_MCP_AUTH_ADMIN_EMAIL",
    ),
];

/// Apply the Claude Code plugin-option → `AXON_*` env-var mapping.
///
/// CRITICAL ORDERING: this MUST run before `Config::load` / `parse_args`, which
/// reads the `AXON_*` env vars. It is therefore invoked from the very top of
/// `axon::run()` (before `parse_args()`), gated to the `setup plugin-hook`
/// invocation. The bash hook used to `export` these before exec'ing `axon`;
/// porting the mapping into the binary preserves that pre-Config timing.
///
/// Values containing newlines or carriage returns are skipped (mirrors the
/// bash `reject_unsafe_value` guard) to avoid env-var injection. Empty values
/// are skipped so they don't clobber values from `.env` / `config.toml`.
#[allow(unsafe_code)]
pub fn apply_plugin_options() {
    for (option_env, axon_env) in PLUGIN_OPTION_MAPPINGS {
        let Some(raw) = std::env::var_os(option_env) else {
            continue;
        };
        // Skip values with embedded newlines/CR — defense against env injection.
        let bytes = raw.as_encoded_bytes();
        if bytes.contains(&b'\n') || bytes.contains(&b'\r') {
            eprintln!("axon plugin setup: {option_env} must not contain newlines; skipping");
            continue;
        }
        // Skip empty values so they don't override .env / config.toml.
        if bytes.is_empty() {
            continue;
        }
        // edition 2024: set_var is unsafe (not thread-safe). This runs single-
        // threaded at the top of run() before any worker threads are spawned.
        unsafe {
            std::env::set_var(axon_env, &raw);
        }
    }

    warn_stale_systemd_unit();
}

/// Advisory warning ported from the bash hook: a leftover user systemd unit can
/// fight the canonical Docker Compose setup for the MCP port. One stderr line.
fn warn_stale_systemd_unit() {
    #[cfg(unix)]
    {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let unit = std::path::Path::new(&home).join(".config/systemd/user/axon-mcp.service");
        if unit.exists() {
            eprintln!(
                "axon plugin setup: stale systemd unit detected at {}; Docker setup is canonical, remove the unit to avoid port conflicts",
                unit.display()
            );
        }
    }
}
