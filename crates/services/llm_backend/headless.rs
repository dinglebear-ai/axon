pub mod claude;
pub mod codex;
pub mod common;
pub mod dispatch;
pub mod gemini;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessAgent {
    Claude,
    Codex,
    Gemini,
}

impl HeadlessAgent {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
        }
    }
}
