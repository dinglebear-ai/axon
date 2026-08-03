//! Terminal status failure mode for fake job store testing.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum FakeJobWatchMode {
    #[default]
    Success,
    TerminalStatusFailure,
}

impl super::FakeJobWatchStore {
    pub fn with_terminal_status_failure(mut self) -> Self {
        self.mode = FakeJobWatchMode::TerminalStatusFailure;
        self
    }
}
