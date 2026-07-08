use super::{apply_extract_overrides, validate_mcp_collection};
use crate::schema::{ExtractRequest, McpRenderMode};
use axon_core::config::{Config, RenderMode};

#[test]
fn mcp_collection_validation_accepts_safe_names() {
    assert_eq!(
        validate_mcp_collection("docs_v2-2026.main").unwrap(),
        "docs_v2-2026.main"
    );
}

#[test]
fn mcp_collection_validation_rejects_path_and_query_chars() {
    assert!(validate_mcp_collection("../secrets").is_err());
    assert!(validate_mcp_collection("docs/v1").is_err());
    assert!(validate_mcp_collection("docs?token=abc").is_err());
    assert!(validate_mcp_collection("docs#frag").is_err());
    assert!(validate_mcp_collection(".hidden").is_err());
    assert!(validate_mcp_collection("trailing.").is_err());
    assert!(validate_mcp_collection("a..b").is_err());
    assert!(validate_mcp_collection("").is_err());
}

#[test]
fn mcp_extract_overrides_preserve_render_mode_and_embed() {
    let cfg = Config::default_minimal();
    let req = ExtractRequest {
        render_mode: Some(McpRenderMode::Chrome),
        embed: Some(false),
        prompt: Some("extract prices".to_string()),
        max_pages: Some(17),
        ..ExtractRequest::default()
    };

    let cfg = apply_extract_overrides(&cfg, &req);

    assert_eq!(cfg.render_mode, RenderMode::Chrome);
    assert!(!cfg.embed);
    assert_eq!(cfg.query.as_deref(), Some("extract prices"));
    assert_eq!(cfg.max_pages, 17);
}

// --- logged_internal_error ---

use super::logged_internal_error;
use axon_core::error::ServiceTaxonomyError;
use std::error::Error as StdError;
use std::fmt;

/// Leaf error with no source.
#[derive(Debug)]
struct LeafErr(&'static str);
impl fmt::Display for LeafErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl StdError for LeafErr {}

/// Error whose `source()` returns itself — a pathological cyclic chain.
#[derive(Debug)]
struct CyclicErr;
impl fmt::Display for CyclicErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cyclic")
    }
}
impl StdError for CyclicErr {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self)
    }
}

/// Error carrying an optional boxed source, for building multi-link chains.
#[derive(Debug)]
struct ChainedErr {
    msg: &'static str,
    source: Option<Box<dyn StdError + 'static>>,
}
impl fmt::Display for ChainedErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}
impl StdError for ChainedErr {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_deref()
    }
}

#[test]
fn logged_internal_error_includes_top_level_cause_in_client_message() {
    let e = LeafErr("boom");
    let err = logged_internal_error("ask", &e);
    // The client-facing message now carries the actionable cause, not a bare
    // "ask failed". This is the whole point of the change.
    assert_eq!(&*err.message, "ask failed: boom");
}

#[test]
fn logged_internal_error_forwards_only_top_level_display_to_client() {
    // A 3-link chain A -> B -> C. The client message must carry ONLY the
    // top-level Display ("outer"); the deeper chain stays in the server log.
    let inner = ChainedErr {
        msg: "inner-secret",
        source: None,
    };
    let mid = ChainedErr {
        msg: "middle",
        source: Some(Box::new(inner)),
    };
    let outer = ChainedErr {
        msg: "outer",
        source: Some(Box::new(mid)),
    };
    let err = logged_internal_error("query 'x'", &outer);
    assert_eq!(&*err.message, "query 'x' failed: outer");
    // The ": "-joined chain is only observable in the tracing log line, which
    // is not asserted here (the repo has no tracing-capture harness).
    assert!(!err.message.contains("inner-secret"));
    assert!(!err.message.contains("middle"));
}

#[test]
fn logged_internal_error_terminates_on_self_referential_source() {
    // The depth cap must stop the source-chain walk; a self-referential
    // `source()` would otherwise loop forever. Reaching the assertion at all
    // proves termination.
    let err = logged_internal_error("retrieve 'x'", &CyclicErr);
    assert_eq!(&*err.message, "retrieve 'x' failed: cyclic");
}

#[test]
fn logged_internal_error_redacts_secrets_from_message() {
    let e = LeafErr("connection failed: Authorization: Bearer abcdef0123456789abcdef");
    let err = logged_internal_error("ask", &e);
    assert!(!err.message.contains("abcdef0123456789abcdef"));
}

#[test]
fn logged_internal_error_surfaces_service_taxonomy_data() {
    let err = ServiceTaxonomyError::VerticalAuthMissing {
        vertical: "github_repo",
    };
    let data = logged_internal_error("scrape", &err)
        .data
        .expect("taxonomy data");

    assert_eq!(data["error"]["code"], "vertical_auth_missing");
    assert_eq!(data["error"]["retriable"], false);
    assert_eq!(data["error"]["source"], "github_repo");
    assert_eq!(data["error"]["details"]["vertical"], "github_repo");
}
