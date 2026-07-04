//! Test doubles for crates that need document preparation without runtime IO.

use crate::prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use crate::preparer::DocumentPreparer;

#[derive(Debug, Clone)]
pub struct RecordingPreparer {
    inner: DocumentPreparer,
    requests: Vec<PrepareSourceDocumentRequest>,
}

impl RecordingPreparer {
    pub fn new(inner: DocumentPreparer) -> Self {
        Self {
            inner,
            requests: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        self.requests.push(request.clone());
        self.inner.prepare(request)
    }

    pub fn requests(&self) -> &[PrepareSourceDocumentRequest] {
        &self.requests
    }
}
