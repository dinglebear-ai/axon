#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainMode {
    ExplainOnly,
    ExplainWithAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainScoreKind {
    Cosine,
    Rrf,
    NamedDense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainScoreComponentStatus {
    Applied,
    Skipped,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainScoreComponent {
    pub name: String,
    pub value: f64,
    pub status: AskExplainScoreComponentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainRetrieval {
    pub query: String,
    pub keyword_query: String,
    pub dual_search: bool,
    pub collection: String,
    pub candidate_limit: usize,
    pub hybrid_search_enabled: bool,
    pub hybrid_candidate_limit: usize,
    pub score_kind: AskExplainScoreKind,
    pub vector_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_query_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainFilterDecisionKind {
    Kept,
    DroppedLowSignal,
    DroppedMinRelevance,
    DroppedTopicalOverlap,
    DroppedDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainFilterDecision {
    pub kind: AskExplainFilterDecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainSelectionDecisionKind {
    SelectedTopChunk,
    PlannedFullDoc,
    InsertedFullDoc,
    SkippedPlannedFullDoc,
    SkippedFullDocFetchSkipped,
    SelectedSupplemental,
    SkippedBudget,
    NotSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainSelectionDecision {
    pub kind: AskExplainSelectionDecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainInsertionMode {
    TopChunk,
    PlannedFullDoc,
    InsertedFullDoc,
    Supplemental,
    NotSelected,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainCandidate {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_rerank_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_full_doc_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_context_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insertion_mode: Option<AskExplainInsertionMode>,
    pub retrieval_score: f64,
    pub rerank_score: f64,
    pub score_kind: AskExplainScoreKind,
    pub score_components: Vec<AskExplainScoreComponent>,
    pub filter_decisions: Vec<AskExplainFilterDecision>,
    pub selection_decisions: Vec<AskExplainSelectionDecision>,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainContextSource {
    pub source_id: String,
    pub url: String,
    pub tier: AskExplainContextSourceTier,
    #[serde(default)]
    pub sort_rank: usize,
    #[serde(default)]
    pub sort_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainContextSourceTier {
    TopChunk,
    FullDoc,
    Supplemental,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainFullDocFetchMode {
    Cosine,
    Rrf,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainFullDocFetchSkipReason {
    Disabled,
    EmptyTopK,
    InsufficientUrls,
    InsufficientChars,
    LowTopScores,
    OkSkip,
    #[serde(other)]
    Unknown,
}

impl From<&str> for AskExplainFullDocFetchSkipReason {
    fn from(value: &str) -> Self {
        match value {
            "disabled" => Self::Disabled,
            "empty_top_k" => Self::EmptyTopK,
            "insufficient_urls" => Self::InsufficientUrls,
            "insufficient_chars" => Self::InsufficientChars,
            "low_top_scores" => Self::LowTopScores,
            "ok_skip" => Self::OkSkip,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainRenderedContextFormat {
    AxonSourcesV1,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainContextRendered {
    pub format: AskExplainRenderedContextFormat,
    pub content: String,
    pub bytes_used: usize,
    pub chars_used: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainContext {
    pub planned_full_doc_urls: Vec<String>,
    pub full_doc_fetch_skipped: bool,
    pub full_doc_fetch_skip_reason: AskExplainFullDocFetchSkipReason,
    pub full_doc_fetch_mode: AskExplainFullDocFetchMode,
    pub final_source_order: Vec<AskExplainContextSource>,
    /// Deprecated compatibility alias. Runtime budget enforcement is byte-based;
    /// prefer `context_bytes_budget`.
    pub context_char_budget: usize,
    /// Actual Unicode scalar value count for the final rendered context.
    pub context_chars_used: usize,
    #[serde(default)]
    pub context_bytes_budget: usize,
    #[serde(default)]
    pub context_bytes_used: usize,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_rendered_context",
        skip_serializing_if = "Option::is_none"
    )]
    pub rendered_context: Option<AskExplainContextRendered>,
    pub truncated_by_budget: bool,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum AskExplainContextRenderedCompat {
    Structured(AskExplainContextRendered),
    Legacy(String),
}

fn deserialize_optional_rendered_context<'de, D>(
    deserializer: D,
) -> Result<Option<AskExplainContextRendered>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(value) =
        <Option<AskExplainContextRenderedCompat> as serde::Deserialize>::deserialize(deserializer)?
    else {
        return Ok(None);
    };
    Ok(Some(match value {
        AskExplainContextRenderedCompat::Structured(rendered) => rendered,
        AskExplainContextRenderedCompat::Legacy(content) => AskExplainContextRendered {
            bytes_used: content.len(),
            chars_used: content.chars().count(),
            content,
            format: AskExplainRenderedContextFormat::AxonSourcesV1,
        },
    }))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainTrace {
    pub mode: AskExplainMode,
    pub retrieval: AskExplainRetrieval,
    pub candidates: Vec<AskExplainCandidate>,
    pub context: AskExplainContext,
    pub candidate_trace_limit: usize,
    pub candidate_trace_truncated: bool,
    pub llm_skipped: bool,
}
