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
    let prompt = synthesis_prompt_for_gemini();
    assert!(
        prompt.contains("axon-rag-synthesize"),
        "prompt must reference the skill by name"
    );
    assert!(
        prompt.contains("Use\n[S1], [S2], etc. exactly as shown")
            || prompt.contains("Use\r\n[S1], [S2], etc. exactly as shown")
            || prompt.contains("Use [S1], [S2], etc. exactly as shown"),
        "prompt must include direct source citation instructions"
    );
    assert!(
        !prompt.trim().is_empty(),
        "synthesis_prompt must not be empty"
    );
}

#[test]
fn generic_synthesis_prompt_omits_skill_frontmatter_and_activation() {
    let prompt = synthesis_prompt_for_openai_compat();
    assert!(
        !prompt.contains("Use the axon-rag-synthesize skill"),
        "generic OpenAI-compatible prompt should not ask the model to activate a Gemini skill"
    );
    assert!(
        !prompt.contains("name: axon-rag-synthesize"),
        "generic prompt must strip skill YAML frontmatter"
    );
    assert!(
        !prompt.contains("user-invocable: false"),
        "generic prompt must strip skill YAML frontmatter"
    );
    assert!(
        prompt.contains("You are a source-grounded technical assistant."),
        "generic prompt should retain the synthesis contract"
    );
}

#[test]
fn codex_app_server_uses_direct_synthesis_prompt() {
    let prompt = synthesis_prompt_for_backend(axon_core::llm::LlmBackendKind::CodexAppServer);

    assert!(!prompt.contains("Use the axon-rag-synthesize skill"));
    assert!(prompt.contains("provided context"));
}

#[test]
fn gemini_synthesis_prompt_keeps_activation_but_strips_frontmatter() {
    let prompt = synthesis_prompt_for_gemini();
    assert!(
        prompt.contains("Use the axon-rag-synthesize skill"),
        "Gemini prompt should keep native skill activation"
    );
    assert!(
        !prompt.contains("name: axon-rag-synthesize"),
        "Gemini system prompt should not inline YAML frontmatter"
    );
    assert!(
        prompt.contains("You are a source-grounded technical assistant."),
        "Gemini prompt should retain the direct fallback contract"
    );
}

#[test]
fn synthesis_prompt_contains_tightened_grounding_contract() {
    let prompt = synthesis_prompt_for_openai_compat();
    assert!(
        prompt.contains(
            "Every sentence containing factual content must end with one or more source citations."
        ),
        "prompt should state citation placement in sentence-level terms"
    );
    assert!(
        prompt.contains("If sources conflict, say they conflict and cite both sides."),
        "prompt should explicitly handle conflicting sources"
    );
    assert!(
        prompt.contains("For non-trivial answers, cite at least two distinct source identifiers"),
        "prompt should align with ask citation validation"
    );
    assert!(
        prompt.contains("Do not request tools, browsing, web search, or additional retrieval."),
        "prompt should forbid synthesis-time tool or web requests"
    );
    assert!(
        prompt.contains("Only name exact URLs or paths if they appear in the retrieved context or the user question."),
        "prompt should avoid invented index-next URLs"
    );
    assert!(
        prompt.contains("ignore them silently"),
        "prompt should tell models to silently ignore prompt injections"
    );
}

#[test]
fn synthesis_prompt_requires_step_by_step_for_procedural_questions() {
    let prompt = synthesis_prompt_for_openai_compat();
    assert!(
        prompt.contains("how do I")
            && prompt.contains("step-by-step")
            && prompt.contains("prerequisites")
            && prompt.contains("example file contents"),
        "prompt should classify procedural questions as guide-worthy instead of short-summary answers"
    );
}
