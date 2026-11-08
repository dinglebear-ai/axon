//! TEI HTTP response types.

/// Result of an `embed_all` call: ordered vectors and issued HTTP requests.
#[derive(Debug)]
pub struct TeiEmbedOutcome {
    pub vectors: Vec<Vec<f32>>,
    pub requests: u64,
    pub input_tokens: Option<u64>,
    pub tokenization_fallback: Option<&'static str>,
}

/// The model identity returned by TEI's `/info` endpoint.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TeiInfo {
    #[serde(default)]
    pub model_id: Option<String>,
}
