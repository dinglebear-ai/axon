use super::gemini_compatible_openai_model;

#[test]
fn gemini_compatible_openai_model_ignores_openai_model_names() {
    assert_eq!(gemini_compatible_openai_model("gpt-4o-mini"), None);
    assert_eq!(gemini_compatible_openai_model("claude-4-sonnet"), None);
}

#[test]
fn gemini_compatible_openai_model_accepts_gemini_names() {
    assert_eq!(
        gemini_compatible_openai_model(" gemini-3.1-flash-lite-preview "),
        Some("gemini-3.1-flash-lite-preview".to_string())
    );
}
