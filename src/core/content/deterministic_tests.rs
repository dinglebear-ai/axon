use super::{flatten_results, parse_llm_fallback_json, strip_llm_fallback_envelope};

#[test]
fn extract_items_fallback_parses_results_from_llm_json_text() {
    let parsed = serde_json::json!({
        "results": [
            {"title": "first"},
            {"title": "second"}
        ]
    });
    let mut out = Vec::new();
    flatten_results(&parsed, &mut out);
    assert_eq!(out.len(), 2);
}

#[test]
fn strip_envelope_handles_json_code_fence() {
    let raw = "```json\n{\"results\":[{\"a\":1}]}\n```";
    assert_eq!(
        strip_llm_fallback_envelope(raw),
        "{\"results\":[{\"a\":1}]}"
    );
}

#[test]
fn strip_envelope_handles_bare_code_fence() {
    let raw = "```\n[1, 2, 3]\n```";
    assert_eq!(strip_llm_fallback_envelope(raw), "[1, 2, 3]");
}

#[test]
fn strip_envelope_skips_leading_prose() {
    let raw = "Model switched to claude-sonnet-4-6. Ready to help.\n{\"results\":[]}";
    assert_eq!(strip_llm_fallback_envelope(raw), "{\"results\":[]}");
}

#[test]
fn parse_llm_fallback_recovers_fenced_json() {
    let raw = "```json\n{\"results\":[{\"title\":\"x\"}]}\n```";
    let v = parse_llm_fallback_json(raw).expect("must parse fenced JSON");
    assert_eq!(v["results"][0]["title"], "x");
}

#[test]
fn parse_llm_fallback_recovers_prose_prefixed_json() {
    let raw = "Greetings — extracted items below:\n[{\"title\":\"y\"}]";
    let v = parse_llm_fallback_json(raw).expect("must parse prose-prefixed JSON");
    assert_eq!(v[0]["title"], "y");
}
