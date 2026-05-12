use std::sync::LazyLock;

// Compile-time embedded default — always available, no filesystem dependency.
// Runtime override: AXON_RAG_SYNTHESIZE_SKILL_PATH env var or
// $AXON_DATA_DIR/skills/axon-rag-synthesize/SKILL.md
const EMBEDDED_SKILL: &str =
    include_str!("../../../../../plugins/skills/axon-rag-synthesize/SKILL.md");

/// Fallback constant — the compiled-in synthesis prompt after frontmatter strip.
/// Retained as a named export so existing tests referencing it by name still compile.
pub(crate) static ASK_RAG_SYSTEM_PROMPT: LazyLock<&'static str> =
    LazyLock::new(|| strip_yaml_frontmatter(EMBEDDED_SKILL).leak());

static SYNTHESIS_PROMPT: LazyLock<&'static str> =
    LazyLock::new(|| try_load_runtime_override().unwrap_or(*ASK_RAG_SYSTEM_PROMPT));

pub(crate) fn synthesis_prompt() -> &'static str {
    &SYNTHESIS_PROMPT
}

fn try_load_runtime_override() -> Option<&'static str> {
    let path = resolve_override_path()?;
    let canonical = std::fs::canonicalize(&path).ok()?;
    let data_dir = resolve_data_dir();
    let data_dir_canonical = std::fs::canonicalize(&data_dir).ok()?;
    if !canonical.starts_with(&data_dir_canonical) {
        tracing::warn!(
            path = %canonical.display(),
            "ask synthesis: skill path outside AXON_DATA_DIR — ignoring override"
        );
        return None;
    }
    if std::fs::symlink_metadata(&path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        tracing::warn!(
            path = %path.display(),
            "ask synthesis: skill path is a symlink — ignoring"
        );
        return None;
    }
    let content = std::fs::read_to_string(&canonical).ok()?;
    const MAX_BYTES: usize = 256 * 1024;
    if content.len() > MAX_BYTES {
        tracing::warn!(
            bytes = content.len(),
            max = MAX_BYTES,
            "ask synthesis: skill file too large — using compiled default"
        );
        return None;
    }
    let body = strip_yaml_frontmatter(&content);
    if body.trim().is_empty() {
        tracing::warn!("ask synthesis: skill file body is empty — using compiled default");
        return None;
    }
    tracing::info!(
        source = %canonical.display(),
        "ask synthesis prompt loaded from runtime override"
    );
    Some(body.leak())
}

fn resolve_override_path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var("AXON_RAG_SYNTHESIZE_SKILL_PATH")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        return Some(std::path::PathBuf::from(p));
    }
    let candidate = resolve_data_dir()
        .join("skills")
        .join("axon-rag-synthesize")
        .join("SKILL.md");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_data_dir() -> std::path::PathBuf {
    std::env::var("AXON_DATA_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            crate::core::paths::axon_home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join(".axon")
        })
}

pub(super) fn strip_yaml_frontmatter(content: &str) -> String {
    if !content.starts_with("---") {
        return content.to_string();
    }
    let rest = &content[3..];
    if let Some(pos) = rest.find("\n---") {
        rest[pos + 4..].trim_start().to_string()
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_frontmatter_removes_yaml_block() {
        let input = "---\nname: test\ndescription: foo\n---\nActual body content here.";
        assert_eq!(strip_yaml_frontmatter(input), "Actual body content here.");
    }

    #[test]
    fn strip_frontmatter_no_frontmatter_returns_full_content() {
        let input = "No frontmatter here, just content.";
        assert_eq!(strip_yaml_frontmatter(input), input);
    }

    #[test]
    fn strip_frontmatter_malformed_single_dash_returns_full_content() {
        let input = "---\nname: test\nno closing dashes";
        assert_eq!(strip_yaml_frontmatter(input), input);
    }

    #[test]
    fn strip_frontmatter_empty_body_returns_empty() {
        let input = "---\nname: test\n---\n   ";
        assert_eq!(strip_yaml_frontmatter(input).trim(), "");
    }

    #[test]
    fn embedded_skill_is_non_empty_after_strip() {
        let body = strip_yaml_frontmatter(EMBEDDED_SKILL);
        assert!(
            !body.trim().is_empty(),
            "Embedded skill body must not be empty after frontmatter strip"
        );
        assert!(
            body.len() > 100,
            "Embedded skill body must be substantial (got {} chars)",
            body.len()
        );
    }

    #[test]
    fn embedded_skill_has_no_blanket_concise_instruction() {
        let body = strip_yaml_frontmatter(EMBEDDED_SKILL);
        assert!(
            !body.contains("Provide a concise answer"),
            "Skill must not contain the blanket 'Provide a concise answer' instruction"
        );
    }

    #[test]
    fn synthesis_prompt_returns_non_empty_string() {
        let prompt = synthesis_prompt();
        assert!(
            !prompt.trim().is_empty(),
            "synthesis_prompt() must never return empty string"
        );
        assert!(
            prompt.len() > 50,
            "synthesis_prompt() must return substantial content"
        );
    }
}
