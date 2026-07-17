use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestMemorySubaction {
    Remember,
    List,
    Search,
    Show,
    Link,
    Supersede,
    Context,
    Reinforce,
    Contradict,
    Pin,
    Archive,
    Forget,
    Review,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestMemoryNodeType {
    Decision,
    Fact,
    Preference,
    Task,
    Bug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestMemoryEdgeType {
    RelatesTo,
    Supersedes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestMemoryRequest {
    pub subaction: Option<RestMemorySubaction>,
    pub id: Option<String>,
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub edge_type: Option<RestMemoryEdgeType>,
    pub memory_type: Option<RestMemoryNodeType>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub query: Option<String>,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub file: Option<String>,
    pub status: Option<String>,
    pub confidence: Option<f64>,
    pub limit: Option<usize>,
    pub depth: Option<usize>,
    pub token_budget: Option<usize>,
    pub amount: Option<f64>,
    pub pinned: Option<bool>,
    pub reason: Option<String>,
    pub memory_ids: Option<Vec<String>>,
    pub strategy: Option<String>,
    pub archive_sources: Option<bool>,
}

impl From<RestMemorySubaction> for axon_api::mcp_schema::MemorySubaction {
    fn from(value: RestMemorySubaction) -> Self {
        match value {
            RestMemorySubaction::Remember => Self::Remember,
            RestMemorySubaction::List => Self::List,
            RestMemorySubaction::Search => Self::Search,
            RestMemorySubaction::Show => Self::Show,
            RestMemorySubaction::Link => Self::Link,
            RestMemorySubaction::Supersede => Self::Supersede,
            RestMemorySubaction::Context => Self::Context,
            RestMemorySubaction::Reinforce => Self::Reinforce,
            RestMemorySubaction::Contradict => Self::Contradict,
            RestMemorySubaction::Pin => Self::Pin,
            RestMemorySubaction::Archive => Self::Archive,
            RestMemorySubaction::Forget => Self::Forget,
            RestMemorySubaction::Review => Self::Review,
            RestMemorySubaction::Compact => Self::Compact,
        }
    }
}

impl From<RestMemoryNodeType> for axon_api::mcp_schema::MemoryNodeType {
    fn from(value: RestMemoryNodeType) -> Self {
        match value {
            RestMemoryNodeType::Decision => Self::Decision,
            RestMemoryNodeType::Fact => Self::Fact,
            RestMemoryNodeType::Preference => Self::Preference,
            RestMemoryNodeType::Task => Self::Task,
            RestMemoryNodeType::Bug => Self::Bug,
        }
    }
}

impl From<RestMemoryEdgeType> for axon_api::mcp_schema::MemoryEdgeType {
    fn from(value: RestMemoryEdgeType) -> Self {
        match value {
            RestMemoryEdgeType::RelatesTo => Self::RelatesTo,
            RestMemoryEdgeType::Supersedes => Self::Supersedes,
        }
    }
}

impl From<RestMemoryRequest> for axon_api::mcp_schema::MemoryRequest {
    fn from(req: RestMemoryRequest) -> Self {
        Self {
            subaction: req.subaction.map(Into::into),
            id: req.id,
            source_id: req.source_id,
            target_id: req.target_id,
            edge_type: req.edge_type.map(Into::into),
            memory_type: req.memory_type.map(Into::into),
            title: req.title,
            body: req.body,
            query: req.query,
            project: req.project,
            repo: req.repo,
            file: req.file,
            status: req.status,
            confidence: req.confidence,
            salience: None,
            limit: req.limit,
            depth: req.depth,
            token_budget: req.token_budget,
            response_mode: None,
            amount: req.amount,
            pinned: req.pinned,
            reason: req.reason,
            memory_ids: req.memory_ids,
            strategy: req.strategy,
            archive_sources: req.archive_sources,
            // Import/export use their dedicated typed request contracts.
            records: None,
            import_mode: None,
            dry_run: None,
            export_scope: None,
            include_archived: None,
            include_working: None,
        }
    }
}
