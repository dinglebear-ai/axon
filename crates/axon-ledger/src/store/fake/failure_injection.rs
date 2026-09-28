//! Focused fake failures for post-creation finalization regression tests.
use super::*;
impl FakeLedgerStore {
    pub fn with_manifest_write_failure(mut self) -> Self {
        self.mode = FakeLedgerMode::ManifestWriteFailure;
        self
    }
    pub fn with_fail_generation_failure(mut self) -> Self {
        self.mode = FakeLedgerMode::FailGenerationFailure;
        self
    }
    pub(super) fn inject_failure(&self, mode: FakeLedgerMode, operation: &str) -> Result<()> {
        if self.mode == mode && self.injected_failure_enabled.load(Ordering::Acquire) {
            return Err(ApiError::new(
                format!("ledger.{operation}_failed"),
                ErrorStage::Cleaning,
                format!("injected {operation} failure"),
            ));
        }
        Ok(())
    }
}
