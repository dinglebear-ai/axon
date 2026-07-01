use axon_api::source::*;

pub type ParseResult = axon_api::source::ParseResult;

#[derive(Debug, Clone)]
pub struct ParseInput {
    pub job_id: JobId,
    pub stage_id: StageId,
    pub document: SourceDocument,
    pub requested_parser: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserCapability {
    pub parser_id: String,
    pub parser_version: String,
    pub content_kinds: Vec<ContentKind>,
    pub mime_types: Vec<String>,
    pub file_extensions: Vec<String>,
    pub path_suffixes: Vec<String>,
    pub sniff_prefixes: Vec<String>,
    pub priority: u32,
}

impl ParserCapability {
    pub fn matches_content_kind(&self, input: &ParseInput) -> bool {
        self.content_kinds.contains(&input.document.content_kind)
    }

    pub fn matches_mime_type(&self, input: &ParseInput) -> bool {
        input
            .document
            .mime_type
            .as_deref()
            .is_some_and(|mime| self.mime_types.iter().any(|candidate| candidate == mime))
    }

    pub fn matches_path(&self, input: &ParseInput) -> bool {
        let Some(path) = input.document.path.as_deref() else {
            return false;
        };
        let extension = path.rsplit('.').next().unwrap_or_default();
        self.file_extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
            || self
                .path_suffixes
                .iter()
                .any(|suffix| path.ends_with(suffix))
    }

    pub fn matches_sniffing(&self, input: &ParseInput) -> bool {
        let ContentRef::InlineText { text } = &input.document.content else {
            return false;
        };
        let trimmed = text.trim_start();
        self.sniff_prefixes
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
    }
}

pub trait SourceParser: Send + Sync {
    fn capability(&self) -> &ParserCapability;

    fn parse(&self, input: &ParseInput) -> ParseResult;
}

pub fn stage_header(
    input: &ParseInput,
    status: LifecycleStatus,
    warnings: Vec<SourceWarning>,
    error: Option<SourceError>,
) -> StageResultHeader {
    StageResultHeader {
        job_id: input.job_id,
        stage_id: input.stage_id,
        phase: PipelinePhase::Parsing,
        status,
        started_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
        completed_at: Some(Timestamp("2026-07-01T00:00:00Z".to_string())),
        counts: StageCounts {
            items_total: Some(1),
            items_done: 1,
            documents_total: Some(1),
            documents_done: 1,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: inline_text_len(&input.document) as u64,
        },
        warnings,
        error,
    }
}

pub fn inline_text_len(document: &SourceDocument) -> usize {
    match &document.content {
        ContentRef::InlineText { text } => text.len(),
        _ => 0,
    }
}
