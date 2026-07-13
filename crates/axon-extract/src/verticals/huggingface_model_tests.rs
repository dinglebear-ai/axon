use super::*;

#[test]
fn test_matches_model_url() {
    assert!(matches("https://huggingface.co/meta-llama/Llama-2-7b-hf"));
    assert!(matches("https://huggingface.co/openai/whisper-large"));
    // Reserved namespaces should not match
    assert!(!matches("https://huggingface.co/datasets/squad"));
    assert!(!matches("https://huggingface.co/spaces/gradio/hello_world"));
    assert!(!matches("https://huggingface.co/blog/llama2"));
}

#[test]
fn test_build_extra_fields() {
    let tags = vec!["text-generation", "transformers", "llama"];
    let extra = build_extra(
        "meta-llama/Llama-2-7b-hf",
        "meta-llama",
        "text-generation",
        "transformers",
        500_000,
        12_000,
        &tags,
    );
    assert_eq!(extra["hf_model_id"], "meta-llama/Llama-2-7b-hf");
    assert_eq!(extra["hf_org"], "meta-llama");
    assert_eq!(extra["hf_downloads"], 500_000u64);
    assert_eq!(extra["hf_likes"], 12_000u64);
    assert_eq!(extra["hf_task"], "text-generation");
    assert_eq!(extra["hf_library"], "transformers");
    assert_eq!(extra["hf_tags"].as_array().unwrap().len(), 3);

    // Empty optional fields should not appear
    let extra_minimal = build_extra("org/model", "org", "", "", 0, 0, &[]);
    assert!(extra_minimal.get("hf_task").is_none());
    assert!(extra_minimal.get("hf_library").is_none());
    assert!(extra_minimal.get("hf_tags").is_none());
}
