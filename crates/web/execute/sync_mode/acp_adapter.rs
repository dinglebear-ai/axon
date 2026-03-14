use std::env;
use std::path::{Path, PathBuf};

use crate::crates::core::config::Config;
use crate::crates::services::acp::validate_adapter_command;
use crate::crates::services::types::AcpAdapterCommand;

use super::types::PulseChatAgent;

/// Delimiter for `AXON_ACP_ADAPTER_ARGS`.
///
/// The env var is parsed as a pipe-delimited list (e.g. `--flag|value|--stdio`).
/// Empty segments are ignored.
const ACP_ADAPTER_ARGS_DELIMITER: char = '|';

/// Parse pipe-delimited adapter args from `AXON_ACP_ADAPTER_ARGS`.
fn parse_acp_adapter_args(raw: &str) -> Vec<String> {
    raw.split(ACP_ADAPTER_ARGS_DELIMITER)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Capability flags threaded from the client WS request into the ACP adapter command.
pub(super) struct AdapterCapabilities {
    pub enable_fs: bool,
    pub enable_terminal: bool,
    pub permission_timeout_secs: Option<u64>,
    pub adapter_timeout_secs: Option<u64>,
}

fn resolve_acp_adapter_command_from_values(
    cmd_value: Option<&str>,
    args_value: Option<&str>,
    caps: AdapterCapabilities,
) -> Result<AcpAdapterCommand, String> {
    let program = cmd_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "missing required env var AXON_ACP_ADAPTER_CMD for pulse_chat".to_string()
        })?;

    let program =
        resolve_local_executable_path(program, args_value).unwrap_or_else(|| program.to_string());
    let args = args_value.map(parse_acp_adapter_args).unwrap_or_default();

    Ok(AcpAdapterCommand {
        program,
        args,
        cwd: None,
        enable_fs: caps.enable_fs,
        enable_terminal: caps.enable_terminal,
        permission_timeout_secs: caps.permission_timeout_secs,
        adapter_timeout_secs: caps.adapter_timeout_secs,
    })
}

fn default_adapter_for_agent(agent: PulseChatAgent) -> (&'static str, Option<&'static str>) {
    match agent {
        PulseChatAgent::Claude => ("claude-agent-acp", None),
        PulseChatAgent::Codex => ("codex-acp", None),
        PulseChatAgent::Gemini => ("gemini", Some("--experimental-acp")),
    }
}

/// Resolve ACP adapter command and args for `pulse_chat`.
///
/// Values are parsed from `Config` fields sourced from environment parsing:
/// - `Config::acp_adapter_cmd` from `AXON_ACP_ADAPTER_CMD` (required)
/// - `Config::acp_adapter_args` from `AXON_ACP_ADAPTER_ARGS` (optional)
fn candidate_local_executable_paths(program: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if program.contains(std::path::MAIN_SEPARATOR)
        || (cfg!(windows) && program.contains('/'))
        || Path::new(program).is_absolute()
    {
        candidates.push(PathBuf::from(program));
        return candidates;
    }

    if let Some(path_var) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path_var).map(|dir| dir.join(program)));
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join(".local/bin").join(program));
        candidates.push(home.join(".cargo/bin").join(program));
    }

    candidates.push(PathBuf::from("/usr/local/bin").join(program));
    candidates.push(PathBuf::from("/usr/bin").join(program));
    candidates
}

fn resolve_local_executable_path(program: &str, args_value: Option<&str>) -> Option<String> {
    let path = Path::new(program);
    if path.is_absolute() && path.exists() {
        return Some(program.to_string());
    }

    if let Some(found) = candidate_local_executable_paths(program)
        .into_iter()
        .find(|candidate| candidate.exists())
        .map(|candidate| candidate.to_string_lossy().into_owned())
    {
        return Some(found);
    }

    let looks_like_codex = program.contains("codex")
        || args_value
            .map(|args| args.to_ascii_lowercase().contains("codex"))
            .unwrap_or(false);

    if looks_like_codex {
        return candidate_local_executable_paths("codex")
            .into_iter()
            .find(|candidate| candidate.exists())
            .map(|candidate| candidate.to_string_lossy().into_owned());
    }

    let looks_like_gemini = program.contains("gemini")
        || args_value
            .map(|args| args.to_ascii_lowercase().contains("gemini"))
            .unwrap_or(false);

    if looks_like_gemini {
        return candidate_local_executable_paths("gemini")
            .into_iter()
            .find(|candidate| candidate.exists())
            .map(|candidate| candidate.to_string_lossy().into_owned());
    }

    None
}

pub(super) fn resolve_acp_adapter_command(
    cfg: &Config,
    agent: PulseChatAgent,
    caps: AdapterCapabilities,
) -> Result<AcpAdapterCommand, String> {
    let (cmd_env_key, args_env_key) = match agent {
        PulseChatAgent::Claude => (
            "AXON_ACP_CLAUDE_ADAPTER_CMD",
            "AXON_ACP_CLAUDE_ADAPTER_ARGS",
        ),
        PulseChatAgent::Codex => ("AXON_ACP_CODEX_ADAPTER_CMD", "AXON_ACP_CODEX_ADAPTER_ARGS"),
        PulseChatAgent::Gemini => (
            "AXON_ACP_GEMINI_ADAPTER_CMD",
            "AXON_ACP_GEMINI_ADAPTER_ARGS",
        ),
    };

    let cmd_override = env::var(cmd_env_key).ok();
    let args_override = env::var(args_env_key).ok();
    let (default_cmd, default_args) = default_adapter_for_agent(agent);

    let cmd = resolve_acp_adapter_command_from_values(
        cmd_override
            .as_deref()
            .or(cfg.acp_adapter_cmd.as_deref())
            .or(Some(default_cmd)),
        args_override
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or(cfg.acp_adapter_args.as_deref())
            .or(default_args),
        caps,
    )?;

    // Run the shell-blocklist and path validation eagerly here so callers that
    // do not go through `AcpClientScaffold::prepare_initialize` cannot bypass
    // the security checks in `validate_adapter_command`.
    validate_adapter_command(&cmd).map_err(|e| e.to_string())?;

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_acp_adapter_args_uses_pipe_delimiter_and_trims_segments() {
        let parsed = parse_acp_adapter_args(" --stdio | --model | gemini-3-flash-preview |  ");
        assert_eq!(parsed, vec!["--stdio", "--model", "gemini-3-flash-preview"]);
    }

    #[test]
    fn parse_acp_adapter_args_returns_empty_for_blank_input() {
        let parsed = parse_acp_adapter_args("   |   || ");
        assert!(parsed.is_empty());
    }

    fn default_caps() -> AdapterCapabilities {
        AdapterCapabilities {
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        }
    }

    #[test]
    fn resolve_acp_adapter_command_reads_required_cmd_and_optional_args() {
        let cmd = resolve_acp_adapter_command_from_values(
            Some("/usr/local/bin/acp-adapter-test"),
            Some("--stdio|--model|gpt-5-mini"),
            default_caps(),
        )
        .expect("env values should resolve");
        assert_eq!(cmd.program, "/usr/local/bin/acp-adapter-test");
        assert_eq!(cmd.args, vec!["--stdio", "--model", "gpt-5-mini"]);
        assert_eq!(cmd.cwd, None);
    }

    #[test]
    fn resolve_acp_adapter_command_requires_non_empty_cmd() {
        let err = resolve_acp_adapter_command_from_values(Some("   "), None, default_caps())
            .expect_err("blank cmd should fail");
        assert!(
            err.contains("AXON_ACP_ADAPTER_CMD"),
            "error should mention missing/invalid env var: {err}"
        );
    }

    #[test]
    fn resolve_acp_adapter_command_requires_cmd_env_var() {
        let err = resolve_acp_adapter_command_from_values(None, None, default_caps())
            .expect_err("missing cmd should fail");
        assert!(
            err.contains("AXON_ACP_ADAPTER_CMD"),
            "error should mention missing/invalid env var: {err}"
        );
    }

    #[test]
    fn resolve_acp_adapter_command_threads_capability_flags() {
        let caps = AdapterCapabilities {
            enable_fs: false,
            enable_terminal: false,
            permission_timeout_secs: Some(120),
            adapter_timeout_secs: Some(600),
        };
        let cmd = resolve_acp_adapter_command_from_values(
            Some("/usr/local/bin/acp-adapter-test"),
            None,
            caps,
        )
        .expect("env values should resolve");
        assert!(!cmd.enable_fs);
        assert!(!cmd.enable_terminal);
        assert_eq!(cmd.permission_timeout_secs, Some(120));
        assert_eq!(cmd.adapter_timeout_secs, Some(600));
    }

    #[test]
    fn default_adapter_for_agent_returns_expected_values() {
        let (claude_cmd, claude_args) = default_adapter_for_agent(PulseChatAgent::Claude);
        assert_eq!(claude_cmd, "claude-agent-acp");
        assert!(claude_args.is_none());

        let (codex_cmd, codex_args) = default_adapter_for_agent(PulseChatAgent::Codex);
        assert_eq!(codex_cmd, "codex-acp");
        assert!(codex_args.is_none());

        let (gemini_cmd, gemini_args) = default_adapter_for_agent(PulseChatAgent::Gemini);
        assert_eq!(gemini_cmd, "gemini");
        assert_eq!(gemini_args, Some("--experimental-acp"));
    }
}
