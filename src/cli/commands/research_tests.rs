use super::*;
use crate::core::config::CommandKind;

fn make_research_cfg(tavily_key: &str, openai_model: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Research;
    cfg.positional = vec!["test query".to_string()];
    cfg.tavily_api_key = tavily_key.to_string();
    cfg.openai_model = openai_model.to_string();
    cfg
}

#[tokio::test]
async fn test_run_research_rejects_empty_tavily_key() {
    let cfg = make_research_cfg("", "gpt-4o-mini");
    let err = run_research(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("TAVILY_API_KEY"),
        "expected TAVILY_API_KEY error, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_allows_gemini_without_adapter() {
    let mut cfg = make_research_cfg("tvly-key", "");
    cfg.positional = vec![];
    cfg.query = None;
    let err = run_research(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("query"),
        "expected query validation after Gemini prereq skip, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_rejects_missing_query() {
    let mut cfg = make_research_cfg("tvly-key", "gpt-4o-mini");
    cfg.positional = vec![];
    cfg.query = None;
    let err = run_research(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("query"),
        "expected query error, got: {err}"
    );
}

#[test]
fn research_cfg_depth_defaults_to_none() {
    let cfg = make_research_cfg("tvly-key", "gpt-4o-mini");
    assert!(
        cfg.research_depth.is_none(),
        "research_depth should default to None"
    );
}
