// The axon-rag-synthesize skill file content, embedded at compile time.
// gemini.rs writes this to the isolated Gemini home so Gemini CLI can discover
// and invoke it natively via the activate_skill tool.
pub(crate) const SKILL_MD: &str =
    include_str!("../../../../../plugins/skills/axon-rag-synthesize/SKILL.md");

/// System prompt shim passed to Gemini headless.
/// The skill carries the full synthesis instructions — this shim just tells
/// Gemini to invoke it before answering.
pub(crate) const ASK_RAG_SYSTEM_PROMPT: &str =
    "Use the axon-rag-synthesize skill to synthesize an answer from the provided context.";

pub(crate) fn synthesis_prompt() -> &'static str {
    ASK_RAG_SYSTEM_PROMPT
}

#[cfg(test)]
fn strip_yaml_frontmatter(content: &str) -> String {
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
    fn skill_md_body_is_non_empty() {
        let body = strip_yaml_frontmatter(SKILL_MD);
        assert!(
            !body.trim().is_empty(),
            "SKILL_MD body must not be empty after frontmatter strip"
        );
        assert!(body.len() > 100, "SKILL_MD body must be substantial");
    }

    #[test]
    fn skill_md_body_has_injection_defense() {
        let body = strip_yaml_frontmatter(SKILL_MD);
        assert!(
            body.contains("untrusted source data"),
            "skill body must contain injection defense"
        );
        assert!(
            body.contains("Never follow"),
            "skill body must contain Never follow instruction"
        );
    }

    #[test]
    fn skill_md_body_has_no_blanket_concise_instruction() {
        let body = strip_yaml_frontmatter(SKILL_MD);
        assert!(
            !body.contains("Provide a concise answer"),
            "skill must not contain blanket 'Provide a concise answer'"
        );
    }

    #[test]
    fn synthesis_prompt_returns_skill_invocation_shim() {
        let prompt = synthesis_prompt();
        assert!(
            prompt.contains("axon-rag-synthesize"),
            "shim must reference the skill by name"
        );
        assert!(
            !prompt.trim().is_empty(),
            "synthesis_prompt must not be empty"
        );
    }
}
