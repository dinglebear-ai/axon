#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReindexSummary {
    pub indexed_files: usize,
    pub removed_files: usize,
}
