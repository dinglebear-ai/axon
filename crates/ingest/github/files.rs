mod batch;
mod clone;
mod line_range;
mod prepare;

use batch::collect_and_embed_batched;
use clone::clone_repo;
use prepare::{build_file_embed_ctx, collect_indexable_files};

pub use clone::{sanitized_git_stderr, should_retry_unauthenticated_clone};
pub use prepare::next_search_start;

use anyhow::{Result, bail};

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::ingest::progress::PhaseReporter;

use super::{GitHubCommonFields, is_indexable_doc_path};

const PHASE_CLONING: &str = "cloning";
const PHASE_ENUMERATING_FILES: &str = "enumerating_files";
const PHASE_EMBEDDED_FILES: &str = "embedded_files";

/// Clone the repo and embed all indexable files concurrently.
pub async fn embed_files(
    cfg: &Config,
    common: &GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
    reporter: &PhaseReporter,
) -> Result<usize> {
    reporter
        .report(serde_json::json!({
            "phase": PHASE_CLONING,
            "repo": common.repo_slug,
        }))
        .await;

    log_info(&format!(
        "github clone_start repo={} branch={}",
        common.repo_slug, common.default_branch
    ));
    let tmp = clone_repo(common, &common.default_branch, token).await?;
    let repo_root = tmp.path().to_path_buf();

    reporter
        .report(serde_json::json!({
            "phase": PHASE_ENUMERATING_FILES,
            "repo": common.repo_slug,
        }))
        .await;

    let file_items = collect_indexable_files(&repo_root, include_source).await?;
    let files_total = file_items.len();

    log_info(&format!(
        "github clone complete indexable={files_total} repo={}",
        common.repo_slug
    ));

    let ctx = build_file_embed_ctx(cfg, common, repo_root);
    let stats = collect_and_embed_batched(&ctx, file_items, files_total, reporter).await?;

    reporter
        .report(serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": stats.chunks_embedded,
            "file_read_failures": stats.failed_file_reads,
            "embed_batches_failed": stats.failed_batches,
            "embed_files_failed": stats.failed_files,
            "embed_docs_failed": stats.failed_docs,
            "embed_chunks_failed": stats.failed_chunks,
            "phase": PHASE_EMBEDDED_FILES,
        }))
        .await;

    log_info(&format!(
        "github files_embedded total={files_total} read_failed={} batches_failed={} files_failed={} docs_failed={} chunks_failed={} chunks={}",
        stats.failed_file_reads,
        stats.failed_batches,
        stats.failed_files,
        stats.failed_docs,
        stats.failed_chunks,
        stats.chunks_embedded
    ));
    if stats.has_failed_batches() {
        bail!(
            "github file embedding had failed batches: batches_failed={} files_failed={} docs_failed={} chunks_failed={} chunks_embedded={}",
            stats.failed_batches,
            stats.failed_files,
            stats.failed_docs,
            stats.failed_chunks,
            stats.chunks_embedded
        );
    }

    Ok(stats.chunks_embedded)
}

#[cfg(test)]
mod tests {
    use super::super::GitHubCommonFields;
    use super::{next_search_start, should_retry_unauthenticated_clone};
    use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};

    fn github_common(is_private: Option<bool>) -> GitHubCommonFields {
        GitHubCommonFields {
            owner: "owner".to_string(),
            name: "repo".to_string(),
            repo_slug: "owner/repo".to_string(),
            default_branch: "main".to_string(),
            repo_description: None,
            pushed_at: None,
            is_private,
            has_wiki: false,
        }
    }

    #[test]
    fn chunk_text_produces_bounded_content() {
        let chunks = chunk_text(&"x".repeat(5000));
        assert!(chunks.iter().all(|chunk| chunk.len() <= 2200));
        assert!(chunks.len() > 1);
    }

    #[test]
    fn search_start_stays_on_char_boundary_with_multibyte_content() {
        let mut text = String::new();
        text.push_str(&"a".repeat(2000));
        text.push_str("─".repeat(200).as_str());
        text.push_str(&"b".repeat(500));

        let mut search_start = 0usize;
        for chunk in &chunk_text(&text) {
            let byte_offset = text[search_start..]
                .find(chunk.as_str())
                .map(|pos| search_start + pos)
                .unwrap_or(search_start);
            search_start = next_search_start(&text, byte_offset, chunk.len());
            assert!(
                text.is_char_boundary(search_start),
                "search_start {search_start} is not a char boundary"
            );
        }
    }

    #[test]
    fn chunk_code_unknown_ext_falls_back() {
        if let Some(chunks) = chunk_code(&"hello world ".repeat(200), "unknownext") {
            assert!(chunks.iter().all(|chunk| chunk.len() <= 2200));
        }
    }

    #[test]
    fn unauthenticated_clone_retry_respects_visibility_and_auth_errors() {
        assert!(!should_retry_unauthenticated_clone(
            &github_common(Some(true)),
            "remote: Repository not found.\nfatal: Authentication failed",
        ));
        assert!(!should_retry_unauthenticated_clone(
            &github_common(Some(false)),
            "remote: Invalid username or token.\nfatal: Authentication failed",
        ));
        assert!(should_retry_unauthenticated_clone(
            &github_common(Some(false)),
            "error: RPC failed; curl 56 GnuTLS recv error",
        ));
        assert!(!should_retry_unauthenticated_clone(
            &github_common(None),
            "remote: Permission to owner/repo.git denied to user.",
        ));
    }
}
