mod taxonomy;

pub use taxonomy::{ChallengeVendor, ServiceTaxonomyError, taxonomy_from_error};

use crate::core::config::Config;
use serde_json::{Value, json};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

/// Structured service error with optional diagnostics payload.
///
/// The `message` is safe for user-facing surfaces. `diagnostics` is optional
/// and must only contain operational metadata with secrets redacted.
#[derive(Debug, Clone)]
pub struct ServiceError {
    message: String,
    diagnostics: Option<Value>,
}

impl ServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            diagnostics: None,
        }
    }

    pub fn with_diagnostics(message: impl Into<String>, diagnostics: Value) -> Self {
        Self {
            message: message.into(),
            diagnostics: Some(diagnostics),
        }
    }

    pub fn diagnostics(&self) -> Option<&Value> {
        self.diagnostics.as_ref()
    }

    pub fn vector_dispatch_failure(
        stage: &'static str,
        cfg: &Config,
        query_len: usize,
        search_context: Value,
        err: &dyn StdError,
    ) -> Self {
        let diagnostics = json!({
            "stage": stage,
            "collection": cfg.collection,
            "qdrant_url": safe_qdrant_url(&cfg.qdrant_url),
            "query_len": query_len,
            "mode": {
                "hybrid_search_enabled": cfg.hybrid_search_enabled,
                "hnsw_ef_search": cfg.hnsw_ef_search,
                "hnsw_ef_search_legacy": cfg.hnsw_ef_search_legacy,
            },
            "search_context": search_context,
            "error": err.to_string(),
        });
        Self::with_diagnostics(format!("vector search dispatch: {err}"), diagnostics)
    }
}

fn safe_qdrant_url(raw: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(raw) else {
        return "<invalid qdrant url>".to_string();
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("redacted");
    }
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

impl Display for ServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for ServiceError {}

/// Walk an error/source chain and return the first structured diagnostics payload.
pub fn diagnostics_from_error<'a>(err: &'a (dyn StdError + 'static)) -> Option<&'a Value> {
    let mut cursor = Some(err);
    while let Some(current) = cursor {
        if let Some(service_error) = current.downcast_ref::<ServiceError>()
            && let Some(diag) = service_error.diagnostics()
        {
            return Some(diag);
        }
        cursor = current.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;

    #[test]
    fn vector_dispatch_failure_redacts_qdrant_url_userinfo_and_query() {
        let mut cfg = Config::test_default();
        cfg.qdrant_url = "https://user:secret@example.com:6333/path?token=secret#frag".to_string();

        let err = std::io::Error::other("boom");
        let service_error = ServiceError::vector_dispatch_failure(
            "query_vector_search_dispatch",
            &cfg,
            12,
            json!({"command": "query"}),
            &err,
        );

        let diag = service_error.diagnostics().expect("diagnostics");
        assert_eq!(diag["qdrant_url"], "https://redacted@example.com:6333/path");
        assert!(!diag.to_string().contains("secret"));
    }
}
