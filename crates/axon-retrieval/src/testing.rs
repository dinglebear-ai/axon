//! Deterministic retrieval testing helpers.

use crate::query::{RetrievalRequest, RetrievalResult};

pub const MODULE_NAME: &str = "testing";

#[derive(Debug, Clone)]
pub struct FakeRetrievalEngine {
    result: RetrievalResult,
}

impl FakeRetrievalEngine {
    pub fn new(result: RetrievalResult) -> Self {
        Self { result }
    }

    pub async fn retrieve(&self, _request: RetrievalRequest) -> RetrievalResult {
        self.result.clone()
    }
}
