use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestSessionsIngestOptions {
    pub claude: Option<bool>,
    pub codex: Option<bool>,
    pub gemini: Option<bool>,
    pub project: Option<String>,
}

impl From<RestSessionsIngestOptions> for crate::mcp::schema::SessionsIngestOptions {
    fn from(value: RestSessionsIngestOptions) -> Self {
        Self {
            claude: value.claude,
            codex: value.codex,
            gemini: value.gemini,
            project: value.project,
        }
    }
}
