//! Revision-bound Labby execution context contracts for ask/chat.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoadoutBinding {
    pub integration_id: String,
    pub loadout_id: String,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoadoutResolutionStatus {
    Effective,
    Narrowed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoadoutResolution {
    pub integration_id: String,
    pub loadout_id: String,
    pub requested_revision: u64,
    pub effective_revision: u64,
    pub catalog_generation: String,
    pub execution_context_id: String,
    pub correlation_id: String,
    pub status: LoadoutResolutionStatus,
    pub effective_capability_count: usize,
    pub unavailable_capability_count: usize,
}
