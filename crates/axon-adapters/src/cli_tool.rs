//! CLI tool/script source contract.
//!
//! Metadata-only by default: `resolve_and_acquire` never spawns a process
//! unless `mode == ToolExecutionMode::Execute` *and* the caller both holds
//! execute scope and has allowlisted the resolved command. Execution never
//! goes through a shell (`std::process::Command` with an explicit argv, no
//! `sh -c`), runs with a cleared environment restricted to
//! `env_allowlist`, is bounded by a timeout and an output byte cap, and has
//! its argv/env/stdout/stderr redacted before being returned for
//! persistence — see "Tool Execution Policy" in
//! `docs/pipeline-unification/runtime/security-contract.md`.
//!
//! [`CliToolSourceAdapter`] wires the above contract into the real
//! `discover`/`acquire`/`normalize` `SourceAdapter` pipeline. `discover`
//! stays metadata-only; `acquire` selects [`ToolExecutionMode::Execute`] only
//! when the service layer stamps an execution-auth marker and supplies the
//! allowlist/env/timeout/output-cap policy in validated route options.

mod adapter;
pub(crate) mod exec;
mod metadata;
mod redact;

use exec::{ExecutionOutcome, execute_command};
use redact::redact_text;

pub use adapter::CliToolSourceAdapter;

pub const MODULE_NAME: &str = "cli_tool";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    MetadataOnly,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolSource {
    pub command: String,
    pub argv: Vec<String>,
    pub env_allowlist: Vec<String>,
    pub side_effect_class: String,
    pub timeout_ms: u64,
    pub output_cap_bytes: usize,
    pub audit_metadata: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliToolExecutionConfig {
    pub env_allowlist: Vec<String>,
    pub side_effect_class: Option<String>,
    pub timeout_ms: Option<u64>,
    pub output_cap_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolDocument {
    pub content_kind: &'static str,
    pub redaction_status: &'static str,
    pub artifact_ref: Option<String>,
    /// Redacted stdout (metadata-only mode: a description of the command,
    /// never executed output).
    pub content: String,
    pub exit_code: Option<i32>,
}

/// A lightweight fact about the resolved CLI tool, precursor to a
/// `SourceParseFacts` row once this adapter is wired into the full
/// discover/acquire/normalize `SourceAdapter` pipeline (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolFact {
    pub fact_kind: &'static str,
    pub name: String,
    pub value: String,
}

/// A precursor to a `GraphNodeCandidate` describing the external command
/// this source resolves to. Full `GraphCandidate` assembly (which
/// additionally requires `job_id`/`source_id`/`document_id`) happens at the
/// `SourceAdapter` integration layer — see module docs and the crate-level
/// follow-up note on wiring `cli_tool`/`mcp_tool` into `SourceAdapter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolExternalResourceNode {
    pub node_kind: &'static str,
    pub stable_key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolAcquireResult {
    pub source: CliToolSource,
    pub documents: Vec<CliToolDocument>,
    pub tool_facts: Vec<CliToolFact>,
    pub graph_nodes: Vec<CliToolExternalResourceNode>,
    pub execution_count: usize,
}

fn tool_facts_for(source: &CliToolSource, execution_count: usize) -> Vec<CliToolFact> {
    vec![
        CliToolFact {
            fact_kind: "cli_target",
            name: "command".to_string(),
            value: source.command.clone(),
        },
        CliToolFact {
            fact_kind: "cli_target",
            name: "argv_count".to_string(),
            value: source.argv.len().to_string(),
        },
        CliToolFact {
            fact_kind: "cli_call",
            name: "execution_count".to_string(),
            value: execution_count.to_string(),
        },
    ]
}

fn graph_nodes_for(source: &CliToolSource) -> Vec<CliToolExternalResourceNode> {
    vec![CliToolExternalResourceNode {
        node_kind: "cli_tool",
        stable_key: source.command.clone(),
        label: source.command.clone(),
    }]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for CliToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CliToolError {}

/// Resolves a `tool:<command> [argv...]` source and, in metadata-only mode,
/// describes it without running anything. In `Execute` mode, `has_execute_scope`
/// must be true and `source.command` must appear verbatim in `allowlist`
/// (checked by [`validate_execute_allowed`]) before a real process is
/// spawned via [`execute_command`].
pub fn resolve_and_acquire(
    input: &str,
    mode: ToolExecutionMode,
    has_execute_scope: bool,
    allowlist: &[&str],
) -> Result<CliToolAcquireResult, CliToolError> {
    let allowlist: Vec<String> = allowlist.iter().map(|value| value.to_string()).collect();
    resolve_and_acquire_configured(
        input,
        mode,
        has_execute_scope,
        &allowlist,
        &CliToolExecutionConfig::default(),
    )
}

pub fn resolve_and_acquire_configured(
    input: &str,
    mode: ToolExecutionMode,
    has_execute_scope: bool,
    allowlist: &[String],
    config: &CliToolExecutionConfig,
) -> Result<CliToolAcquireResult, CliToolError> {
    let mut source = parse_cli_tool_source(input)?;
    apply_execution_config(&mut source, config);

    if mode == ToolExecutionMode::MetadataOnly {
        let tool_facts = tool_facts_for(&source, 0);
        let graph_nodes = graph_nodes_for(&source);
        return Ok(CliToolAcquireResult {
            documents: vec![CliToolDocument {
                content_kind: "tool_metadata",
                redaction_status: "clean",
                artifact_ref: None,
                content: format!(
                    "command `{}` with {} argv token(s); not executed (metadata-only)",
                    source.command,
                    source.argv.len()
                ),
                exit_code: None,
            }],
            tool_facts,
            graph_nodes,
            source,
            execution_count: 0,
        });
    }

    validate_execute_allowed(&source, has_execute_scope, allowlist)?;
    let outcome = execute_command(&source)?;
    Ok(execution_result(source, outcome))
}

fn execution_result(source: CliToolSource, outcome: ExecutionOutcome) -> CliToolAcquireResult {
    let (redacted_stdout, stdout_redacted) = redact_text(&outcome.stdout);
    let (redacted_stderr, stderr_redacted) = redact_text(&outcome.stderr);
    let redacted = stdout_redacted || stderr_redacted;
    let mut content = redacted_stdout;
    if !redacted_stderr.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str("[stderr] ");
        content.push_str(&redacted_stderr);
    }
    let tool_facts = tool_facts_for(&source, 1);
    let graph_nodes = graph_nodes_for(&source);
    CliToolAcquireResult {
        documents: vec![CliToolDocument {
            content_kind: "tool_output",
            redaction_status: if redacted { "redacted" } else { "clean" },
            artifact_ref: None,
            content,
            exit_code: outcome.exit_code,
        }],
        tool_facts,
        graph_nodes,
        source,
        execution_count: 1,
    }
}

/// Real allowlist validator — must be called before any execution path.
/// Denies execution unless the caller holds execute scope, an allowlist was
/// actually supplied, and `source.command` matches an allowlist entry
/// verbatim (no globbing, no prefix matching — the allowlist is a closed
/// set of exact commands).
fn validate_execute_allowed(
    source: &CliToolSource,
    has_execute_scope: bool,
    allowlist: &[String],
) -> Result<(), CliToolError> {
    if !has_execute_scope {
        return Err(CliToolError {
            code: "auth.scope_required",
            message: "CLI tool execution requires execute scope".to_string(),
        });
    }
    if allowlist.is_empty() {
        return Err(CliToolError {
            code: "tool.allowlist_empty",
            message: "no commands are allowlisted for execution".to_string(),
        });
    }
    if !allowlist.iter().any(|allowed| allowed == &source.command) {
        return Err(CliToolError {
            code: "tool.command_denied",
            message: format!("command `{}` is not allowlisted", source.command),
        });
    }
    if tool_token_is_secret_like_path(&source.command)
        || source
            .argv
            .iter()
            .any(|arg| tool_token_is_secret_like_path(arg))
    {
        return Err(CliToolError {
            code: "security.local_secret_denied",
            message: "secret-like local path denied before tool execution".to_string(),
        });
    }
    Ok(())
}

pub fn parse_cli_tool_source(input: &str) -> Result<CliToolSource, CliToolError> {
    let raw = input
        .strip_prefix("tool:")
        .or_else(|| input.strip_prefix("cli://"))
        .or_else(|| input.strip_prefix("cli:"))
        .unwrap_or(input)
        .trim();
    let mut parts = raw.split_whitespace();
    let Some(command) = parts.next() else {
        return Err(CliToolError {
            code: "tool.command_missing",
            message: "CLI tool source requires a command".to_string(),
        });
    };
    let argv = parts.map(str::to_string).collect::<Vec<_>>();
    Ok(CliToolSource {
        command: command.to_string(),
        argv,
        // Empty by design: the default execution environment is fully
        // cleared (`Command::env_clear`). Callers that need specific
        // variables passed through must extend this list explicitly — see
        // "environment allowlist" in the security contract.
        env_allowlist: Vec::new(),
        side_effect_class: "none".to_string(),
        timeout_ms: 5_000,
        output_cap_bytes: 64 * 1024,
        audit_metadata: vec![
            (
                "execution_mode".to_string(),
                "metadata_or_explicit".to_string(),
            ),
            ("shell_expansion".to_string(), "disabled".to_string()),
        ],
    })
}

fn apply_execution_config(source: &mut CliToolSource, config: &CliToolExecutionConfig) {
    source.env_allowlist = config.env_allowlist.clone();
    if let Some(side_effect_class) = config.side_effect_class.as_ref() {
        source.side_effect_class = side_effect_class.clone();
    }
    if let Some(timeout_ms) = config.timeout_ms {
        source.timeout_ms = timeout_ms;
    }
    if let Some(output_cap_bytes) = config.output_cap_bytes {
        source.output_cap_bytes = output_cap_bytes;
    }
}

fn tool_token_is_secret_like_path(token: &str) -> bool {
    let trimmed = token.trim();
    if !(trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.starts_with('.')
        || trimmed.contains('/'))
    {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower == ".env"
        || lower.ends_with("/.env")
        || lower.contains("/.ssh/")
        || lower.contains("/.codex/")
        || lower.contains("/.gemini/")
        || lower.contains("browser-profile")
        || lower.contains("cloud")
        || axon_core::redact::is_secret_like(&lower)
}
