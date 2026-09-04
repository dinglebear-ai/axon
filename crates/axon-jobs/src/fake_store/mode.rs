//! Terminal status failure mode for fake job store testing.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum FakeJobWatchMode {
    #[default]
    Success,
    TerminalStatusFailure,
    AppendEventFailure,
}

impl super::FakeJobWatchStore {
    pub fn with_terminal_status_failure(mut self) -> Self {
        self.mode = FakeJobWatchMode::TerminalStatusFailure;
        self
    }

    pub fn with_append_event_failure(mut self) -> Self {
        self.mode = FakeJobWatchMode::AppendEventFailure;
        self
    }
}
