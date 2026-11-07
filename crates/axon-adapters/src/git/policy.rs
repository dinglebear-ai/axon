//! Configurable inventory policy for historical planning/session documents.
use super::{Result, option_string_array};
use axon_api::source::*;

#[derive(Default)]
pub(super) struct GitInventoryPolicy {
    pub(super) exclude_paths: Vec<String>,
    pub(super) include_historical_docs: bool,
}
impl GitInventoryPolicy {
    pub(super) fn from_options(options: &AdapterOptions) -> Result<Self> {
        let include_historical_docs = match options.values.get("include_historical_docs") {
            None => false,
            Some(value) => value.as_bool().ok_or_else(|| ApiError::new("adapter.git.option.invalid",ErrorStage::Routing,
                "include_historical_docs must be a boolean; use true to include planning/session documents"))?,
        };
        Ok(Self {
            exclude_paths: option_string_array(options, "exclude_paths")?,
            include_historical_docs,
        })
    }
    pub(super) fn allows(&self, path: &str) -> bool {
        if !super::discovery::repository_path_allowed(path, &self.exclude_paths) {
            return false;
        }
        let extension = path
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let historical = matches!(
            extension.as_str(),
            "md" | "mdx" | "markdown" | "rst" | "txt" | "adoc" | "asciidoc"
        ) && path.split('/').any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "plans" | "plan" | "sessions" | "session" | "session-logs" | "session_logs"
            )
        });
        self.include_historical_docs || !historical
    }
}
