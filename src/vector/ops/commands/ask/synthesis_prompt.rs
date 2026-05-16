// The axon-rag-synthesize skill file content, embedded at compile time.
// gemini.rs writes this to the isolated Gemini home so Gemini CLI can discover
// and invoke it natively via the activate_skill tool.
pub(crate) const SKILL_MD: &str =
    include_str!("../../../../../plugins/skills/axon-rag-synthesize/SKILL.md");

/// System prompt passed to Gemini headless.
///
/// Keep the skill invocation for clients that support native skill activation,
/// but inline the full contract as well. Headless model runs are not guaranteed
/// to activate the skill, and uncited answers are rejected by the ask
/// normalizer, so the citation contract must be present in the prompt itself.
pub(crate) const ASK_RAG_SYSTEM_PROMPT: &str = concat!(
    "Use the axon-rag-synthesize skill to synthesize an answer from the provided context.\n\n",
    "You must also follow these instructions directly if the skill is unavailable:\n\n",
    include_str!("../../../../../plugins/skills/axon-rag-synthesize/SKILL.md")
);

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
#[path = "synthesis_prompt_tests.rs"]
mod tests;
