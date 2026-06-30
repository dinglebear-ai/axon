use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use axon_core::content::redact_url;
use axon_core::logging::log_warn;
use axon_vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use axon_vector::ops::{PreparedDoc, SourceDocument, SourceOrigin, prepare_source_document};

use crate::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::subprocess::MAX_INGEST_FILE_BYTES;

use super::GenericGitTarget;

pub(crate) async fn file_docs(
    root: &Path,
    target: &GenericGitTarget,
    branch: &str,
    file: PathBuf,
    source_type: &str,
    provider: &str,
) -> Result<Vec<PreparedDoc>> {
    let rel = file
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");

    match tokio::fs::metadata(&file).await {
        Ok(meta) if meta.len() > MAX_INGEST_FILE_BYTES => {
            log_warn(&format!(
                "command=ingest_git skip_large_file path={rel} size_bytes={}",
                meta.len()
            ));
            return Ok(Vec::new());
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git stat_failed path={rel} err={e}"
            ));
            return Ok(Vec::new());
        }
        _ => {}
    }

    let bytes = match tokio::fs::read(&file).await {
        Ok(b) => b,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git read_failed path={rel} err={e}"
            ));
            return Ok(Vec::new());
        }
    };
    let content = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            log_warn(&format!("command=ingest_git skip_non_utf8 path={rel}"));
            return Ok(Vec::new());
        }
    };

    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let ext = path_extension(&rel).to_ascii_lowercase();
    let lang = language_name(&ext).to_string();
    let ftype = classify_file_type(&rel).to_string();
    let is_test = is_test_path(&rel);
    let extra = build_git_payload(&GitPayload {
        provider: provider.to_string(),
        host: target.host.clone(),
        owner: None,
        repo: target.name.clone(),
        content_kind: ContentKind::File,
        branch: Some(branch.to_string()),
        file_path: Some(rel.clone()),
        file_language: Some(lang),
        file_type: Some(ftype),
        file_is_test: Some(is_test),
        line_start: None,
        line_end: None,
        chunking_method: None,
        symbol_name: None,
        symbol_kind: None,
        symbol_extraction_status: None,
        meta: Some(serde_json::json!({ "clone_url": redact_url(&target.clone_url) })),
        ..GitPayload::default()
    });
    let url = format!("{}#{}:{}", target.web_url, branch, rel);
    let source_doc = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        url,
        rel.clone(),
        ext,
        content,
        source_type,
        Some(rel.clone()),
        Some(extra),
    )
    .map_err(|err| anyhow!("invalid source document for {rel}: {err}"))?;
    let doc = prepare_source_document(source_doc)
        .await
        .map_err(|err| anyhow!("prepare source document failed for {rel}: {err}"))?;
    Ok(vec![doc])
}
