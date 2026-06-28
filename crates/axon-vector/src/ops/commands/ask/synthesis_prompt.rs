// The rag-synthesize prompt content, embedded at compile time.
// gemini.rs writes this to the isolated Gemini home so Gemini CLI can discover
// and invoke it natively via the activate_skill tool.
pub(crate) const SKILL_MD: &str =
    include_str!("../../../../../../plugins/axon/references/rag-synthesize/SKILL.md");

const GEMINI_SKILL_INVOCATION: &str =
    "Use the rag-synthesize skill to synthesize an answer from the provided context.\n\n";
const DIRECT_FALLBACK_INTRO: &str =
    "You must also follow these instructions directly if the skill is unavailable:\n\n";

pub(crate) fn synthesis_prompt_for_gemini() -> String {
    format!(
        "{GEMINI_SKILL_INVOCATION}{DIRECT_FALLBACK_INTRO}{}",
        strip_yaml_frontmatter(SKILL_MD)
    )
}

pub(crate) fn synthesis_prompt_for_openai_compat() -> String {
    strip_yaml_frontmatter(SKILL_MD)
}

pub(crate) fn synthesis_prompt_for_backend(backend: axon_core::llm::LlmBackendKind) -> String {
    match backend {
        axon_core::llm::LlmBackendKind::GeminiHeadless => synthesis_prompt_for_gemini(),
        axon_core::llm::LlmBackendKind::OpenAiCompat => synthesis_prompt_for_openai_compat(),
        axon_core::llm::LlmBackendKind::CodexAppServer => synthesis_prompt_for_openai_compat(),
    }
}

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
