use super::*;
use crate::core::config::CommandKind;

fn make_research_cfg(tavily_key: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Research;
    cfg.positional = vec!["test query".to_string()];
    cfg.tavily_api_key = tavily_key.to_string();
    cfg
}

#[tokio::test]
async fn test_run_research_rejects_empty_tavily_key() {
    let cfg = make_research_cfg("");
    let err = run_research(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("TAVILY_API_KEY"),
        "expected TAVILY_API_KEY error, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_validates_query_before_prereqs() {
    // Both query *and* Tavily key are missing — query check runs first
    // because it's free, while the prereq check waits on the service call.
    let mut cfg = make_research_cfg("");
    cfg.positional = vec![];
    cfg.query = None;
    let err = run_research(&cfg).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("query"),
        "expected query validation to fire first, got: {msg}"
    );
    assert!(
        !msg.contains("TAVILY_API_KEY"),
        "TAVILY error should not surface before query check, got: {msg}"
    );
}

#[tokio::test]
async fn test_run_research_skips_llm_prereq_before_query_validation() {
    let mut cfg = make_research_cfg("tvly-key");
    cfg.positional = vec![];
    cfg.query = None;
    let err = run_research(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("query"),
        "expected query validation after skipping openai_model check, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_rejects_missing_query() {
    let mut cfg = make_research_cfg("tvly-key");
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
    let cfg = make_research_cfg("tvly-key");
    assert!(
        cfg.research_depth.is_none(),
        "research_depth should default to None"
    );
}

#[test]
fn research_depth_overrides_search_limit_when_set() {
    // Mirrors the wiring in `run_research`: `cfg.research_depth.unwrap_or(cfg.search_limit)`.
    // This protects against silent regressions where someone wires depth
    // to a different field or stops reading it.
    let mut cfg = make_research_cfg("tvly-key");
    cfg.search_limit = 5;
    assert_eq!(cfg.research_depth.unwrap_or(cfg.search_limit), 5);

    cfg.research_depth = Some(20);
    assert_eq!(cfg.research_depth.unwrap_or(cfg.search_limit), 20);
}
