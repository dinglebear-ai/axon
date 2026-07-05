//! CLI tool/script source contract.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolDocument {
    pub content_kind: &'static str,
    pub redaction_status: &'static str,
    pub artifact_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliToolAcquireResult {
    pub source: CliToolSource,
    pub documents: Vec<CliToolDocument>,
    pub execution_count: usize,
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

pub fn resolve_and_acquire(
    input: &str,
    mode: ToolExecutionMode,
    has_execute_scope: bool,
    allowlist: &[&str],
) -> Result<CliToolAcquireResult, CliToolError> {
    let source = parse_cli_tool_source(input)?;

    if mode == ToolExecutionMode::Execute {
        if !has_execute_scope {
            return Err(CliToolError {
                code: "auth.scope_required",
                message: "CLI tool execution requires execute scope".to_string(),
            });
        }
        if !allowlist.iter().any(|allowed| *allowed == source.command) {
            return Err(CliToolError {
                code: "tool.command_denied",
                message: format!("command `{}` is not allowlisted", source.command),
            });
        }
    }

    Ok(CliToolAcquireResult {
        source,
        documents: vec![CliToolDocument {
            content_kind: "structured",
            redaction_status: "clean",
            artifact_ref: None,
        }],
        execution_count: usize::from(mode == ToolExecutionMode::Execute),
    })
}

fn parse_cli_tool_source(input: &str) -> Result<CliToolSource, CliToolError> {
    let raw = input.strip_prefix("tool:").unwrap_or(input).trim();
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
