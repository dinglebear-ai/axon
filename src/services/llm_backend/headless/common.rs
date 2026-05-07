pub const STDERR_TAIL_LIMIT: usize = 4096;

const FORBIDDEN_FLAGS: &[&str] = &[
    "--full-auto",
    "--dangerously-bypass-approvals-and-sandbox",
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--yolo",
    "--approval-mode=yolo",
    "danger-full-access",
    "bypassPermissions",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTransport {
    Stdin,
    Argument,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessCommandSpec {
    pub agent: &'static str,
    pub program: String,
    pub args: Vec<String>,
    pub prompt_transport: PromptTransport,
    pub output_mode: &'static str,
}

impl HeadlessCommandSpec {
    pub fn validate(&self) -> Result<(), String> {
        let joined = self.args.join(" ");
        for forbidden in FORBIDDEN_FLAGS {
            if joined.contains(forbidden) {
                return Err(format!(
                    "headless {} command includes forbidden flag {forbidden}",
                    self.agent
                ));
            }
        }
        if self.args.iter().any(|arg| arg == "yolo") {
            return Err(format!(
                "headless {} command includes forbidden yolo approval mode",
                self.agent
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessCommandRequest {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

impl HeadlessCommandRequest {
    #[must_use]
    pub fn new(model: Option<String>, system_prompt: Option<String>) -> Self {
        Self {
            model: non_empty(model),
            system_prompt: non_empty(system_prompt),
        }
    }
}

pub fn env_or_default(var_name: &str, default_program: &str) -> String {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_program.to_string())
}

pub fn redacted_stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_LIMIT);
    let text = String::from_utf8_lossy(&stderr[start..]);
    redact_secrets(&text)
}

pub fn append_bounded_tail(buffer: &mut Vec<u8>, chunk: &[u8]) {
    buffer.extend_from_slice(chunk);
    if buffer.len() > STDERR_TAIL_LIMIT {
        let excess = buffer.len() - STDERR_TAIL_LIMIT;
        buffer.drain(..excess);
    }
}

fn redact_secrets(text: &str) -> String {
    text.split_whitespace()
        .map(|token| {
            if looks_secretish(token) {
                "[REDACTED]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_secretish(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    upper.contains("API_KEY=")
        || upper.contains("TOKEN=")
        || upper.contains("SECRET=")
        || token.starts_with("sk-")
        || token.starts_with("ghp_")
        || token.starts_with("atk_")
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_safety_rejects_forbidden_flags() {
        let spec = HeadlessCommandSpec {
            agent: "codex",
            program: "codex".to_string(),
            args: vec!["exec".to_string(), "--full-auto".to_string()],
            prompt_transport: PromptTransport::Stdin,
            output_mode: "jsonl",
        };
        assert!(spec.validate().is_err());
    }

    #[test]
    fn headless_safety_redacts_and_bounds_stderr() {
        let raw = format!(
            "{} OPENAI_API_KEY=sk-secret TOKEN=atk_token normal",
            "x".repeat(STDERR_TAIL_LIMIT + 64)
        );
        let redacted = redacted_stderr_tail(raw.as_bytes());
        assert!(redacted.len() <= STDERR_TAIL_LIMIT + 128);
        assert!(!redacted.contains("sk-secret"));
        assert!(!redacted.contains("atk_token"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn headless_safety_keeps_only_stderr_tail() {
        let mut buf = Vec::new();
        append_bounded_tail(&mut buf, &vec![b'a'; STDERR_TAIL_LIMIT]);
        append_bounded_tail(&mut buf, b"tail");
        assert_eq!(buf.len(), STDERR_TAIL_LIMIT);
        assert!(buf.ends_with(b"tail"));
    }
}
