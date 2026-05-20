use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    Server,
    Local,
    FallbackLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackOutcome {
    None,
    CompletedEquivalent,
    CompletedDegraded,
    FailedLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteMetadata {
    pub route: RouteKind,
    pub fallback: bool,
    pub fallback_outcome: FallbackOutcome,
    pub capability_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_data_dir: Option<String>,
    #[serde(default)]
    pub effective_endpoints: serde_json::Value,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl RouteMetadata {
    pub fn server(server_url: impl Into<String>) -> Self {
        Self {
            route: RouteKind::Server,
            fallback: false,
            fallback_outcome: FallbackOutcome::None,
            capability_tier: "server".to_string(),
            server_url: Some(server_url.into()),
            local_data_dir: None,
            effective_endpoints: serde_json::json!({}),
            warnings: vec![],
        }
    }
}
