use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommitFailurePoint {
    Serialize,
    Write,
    Flush,
}

pub(super) const REFETCH_TRANSACTIONS_DIR: &str = ".thin-refetch-transactions";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CommitPhase {
    Prepared,
    ManifestCommitted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RefetchCommitJournal {
    pub(super) filename: String,
    pub(super) manifest_start: u64,
    pub(super) phase: CommitPhase,
}

struct PreparedRefetch {
    canonical: String,
    path: std::path::PathBuf,
    tmp_path: std::path::PathBuf,
    line: String,
    journal_path: std::path::PathBuf,
    journal: RefetchCommitJournal,
}

async fn sync_directory(path: &Path) -> std::io::Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all())
        .await
        .map_err(std::io::Error::other)??;
    Ok(())
}

async fn persist_refetch_journal(
    journal_path: &Path,
    journal: &RefetchCommitJournal,
) -> std::io::Result<()> {
    let directory = journal_path
        .parent()
        .ok_or_else(|| std::io::Error::other("refetch journal has no parent"))?;
    tokio::fs::create_dir_all(directory).await?;
    let temporary = journal_path.with_extension("journal-tmp");
    let bytes = serde_json::to_vec(journal).map_err(std::io::Error::other)?;
    tokio::fs::write(&temporary, bytes).await?;
    tokio::fs::File::open(&temporary).await?.sync_all().await?;
    tokio::fs::rename(&temporary, journal_path).await?;
    sync_directory(directory).await
}

pub(super) async fn recover_refetch_commits(output_dir: &Path) {
    let transactions_dir = output_dir.join(REFETCH_TRANSACTIONS_DIR);
    let Ok(mut entries) = tokio::fs::read_dir(&transactions_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let journal_path = entry.path();
        if journal_path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let recovery = async {
            let journal: RefetchCommitJournal =
                serde_json::from_slice(&tokio::fs::read(&journal_path).await?)
                    .map_err(std::io::Error::other)?;
            let filename = Path::new(&journal.filename);
            if filename.file_name() != Some(filename.as_os_str()) {
                return Err(std::io::Error::other("invalid refetch journal filename"));
            }
            let output = output_dir.join("markdown").join(filename);
            let replacement = output.with_extension("refetch-tmp");
            match journal.phase {
                CommitPhase::Prepared => {
                    let manifest_path = output_dir.join("manifest.jsonl");
                    let manifest = tokio::fs::OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .write(true)
                        .open(manifest_path)
                        .await?;
                    manifest.set_len(journal.manifest_start).await?;
                    manifest.sync_data().await?;
                    let _ = tokio::fs::remove_file(&replacement).await;
                }
                CommitPhase::ManifestCommitted => {
                    if tokio::fs::try_exists(&replacement).await? {
                        tokio::fs::rename(&replacement, &output).await?;
                        sync_directory(
                            output
                                .parent()
                                .ok_or_else(|| std::io::Error::other("output has no parent"))?,
                        )
                        .await?;
                    }
                }
            }
            tokio::fs::remove_file(&journal_path).await?;
            sync_directory(&transactions_dir).await
        }
        .await;
        if let Err(error) = recovery {
            log_warn(&format!(
                "thin_refetch: failed to reconcile {}: {error}",
                journal_path.display()
            ));
        }
    }
}

pub(super) async fn write_refetch_results_with_failure(
    mut summary: CrawlSummary,
    results: Vec<RefetchResult>,
    output_dir: &Path,
    failure: Option<CommitFailurePoint>,
) -> CrawlSummary {
    recover_refetch_commits(output_dir).await;
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let Ok(file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest_path)
        .await
    else {
        log_warn("thin_refetch: failed to open manifest for append; skipping disk writes");
        return summary;
    };
    let mut manifest = file;

    for result in results {
        if let Some(ref diagnostic) = result.diagnostic {
            summary.push_diagnostic(diagnostic.clone());
        }
        let manifest_start = manifest
            .metadata()
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        let Some(prepared) =
            prepare_refetch(result, &markdown_dir, output_dir, manifest_start, failure).await
        else {
            continue;
        };
        if !commit_refetch(&mut manifest, &markdown_dir, output_dir, &prepared, failure).await {
            continue;
        }

        // Only report recovery after both durable artifacts succeeded.
        summary.thin_urls.remove(&prepared.canonical);
        summary.thin_pages = summary.thin_pages.saturating_sub(1);
        summary.markdown_files += 1;

        log_info(&format!("thin_refetch: recovered {}", prepared.canonical));
    }

    if let Err(e) = manifest.sync_data().await {
        log_warn(&format!("thin_refetch: manifest flush failed: {e}"));
    }

    summary
}

async fn prepare_refetch(
    result: RefetchResult,
    markdown_dir: &Path,
    output_dir: &Path,
    manifest_start: u64,
    failure: Option<CommitFailurePoint>,
) -> Option<PreparedRefetch> {
    let markdown = result.markdown?;
    let canonical = canonicalize_url_for_dedupe(&result.url)?;
    let filename = url_to_stable_filename(&canonical);
    let path = markdown_dir.join(&filename);
    let tmp_path = path.with_extension("refetch-tmp");
    if tokio::fs::write(&tmp_path, markdown.as_bytes())
        .await
        .is_err()
    {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        log_warn(&format!(
            "thin_refetch: failed to prepare {}",
            path.display()
        ));
        return None;
    }
    let content_hash = hex::encode(Sha256::digest(markdown.as_bytes()));
    let entry = ManifestEntry {
        url: canonical.clone(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars: markdown.len(),
        content_hash: Some(content_hash),
        changed: true,
        structured: None,
    };
    let serialized = if failure == Some(CommitFailurePoint::Serialize) {
        Err(serde_json::Error::io(std::io::Error::other(
            "injected manifest serialization failure",
        )))
    } else {
        serde_json::to_string(&entry)
    };
    let mut line = match serialized {
        Ok(line) => line,
        Err(error) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            log_warn(&format!(
                "thin_refetch: manifest serialize failed for {canonical}: {error}"
            ));
            return None;
        }
    };
    line.push('\n');
    let journal_path = output_dir
        .join(REFETCH_TRANSACTIONS_DIR)
        .join(format!("{}.json", uuid::Uuid::new_v4()));
    let journal = RefetchCommitJournal {
        filename,
        manifest_start,
        phase: CommitPhase::Prepared,
    };
    if let Err(error) = persist_refetch_journal(&journal_path, &journal).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        log_warn(&format!(
            "thin_refetch: failed to journal prepared output for {canonical}: {error}"
        ));
        return None;
    }
    Some(PreparedRefetch {
        canonical,
        path,
        tmp_path,
        line,
        journal_path,
        journal,
    })
}

async fn commit_refetch(
    manifest: &mut tokio::fs::File,
    markdown_dir: &Path,
    output_dir: &Path,
    prepared: &PreparedRefetch,
    failure: Option<CommitFailurePoint>,
) -> bool {
    let write_result = if failure == Some(CommitFailurePoint::Write) {
        Err(std::io::Error::other("injected manifest write failure"))
    } else {
        manifest.write_all(prepared.line.as_bytes()).await
    };
    if let Err(error) = write_result {
        rollback_prepared(manifest, prepared).await;
        log_warn(&format!(
            "thin_refetch: manifest write failed for {}: {error}",
            prepared.canonical
        ));
        return false;
    }
    let flush_result = if failure == Some(CommitFailurePoint::Flush) {
        Err(std::io::Error::other("injected manifest flush failure"))
    } else {
        manifest.flush().await.map(|_| ())
    };
    if flush_result.is_err() || manifest.sync_data().await.is_err() {
        rollback_prepared(manifest, prepared).await;
        log_warn(&format!(
            "thin_refetch: manifest flush failed for {}",
            prepared.canonical
        ));
        return false;
    }
    let mut journal = RefetchCommitJournal {
        phase: CommitPhase::ManifestCommitted,
        ..prepared.journal.clone()
    };
    journal.phase = CommitPhase::ManifestCommitted;
    if let Err(error) = persist_refetch_journal(&prepared.journal_path, &journal).await {
        rollback_prepared(manifest, prepared).await;
        log_warn(&format!(
            "thin_refetch: failed to journal committed manifest for {}: {error}",
            prepared.canonical
        ));
        return false;
    }
    if let Err(error) = tokio::fs::rename(&prepared.tmp_path, &prepared.path).await {
        rollback_prepared(manifest, prepared).await;
        log_warn(&format!(
            "thin_refetch: failed to commit {}: {error}",
            prepared.path.display()
        ));
        return false;
    }
    if let Err(error) = sync_directory(markdown_dir).await {
        log_warn(&format!(
            "thin_refetch: failed to sync committed output for {}: {error}",
            prepared.canonical
        ));
    } else {
        let _ = tokio::fs::remove_file(&prepared.journal_path).await;
        let _ = sync_directory(&output_dir.join(REFETCH_TRANSACTIONS_DIR)).await;
    }
    true
}

async fn rollback_prepared(manifest: &mut tokio::fs::File, prepared: &PreparedRefetch) {
    let _ = manifest.set_len(prepared.journal.manifest_start).await;
    let _ = manifest.sync_data().await;
    let _ = tokio::fs::remove_file(&prepared.tmp_path).await;
    let _ = tokio::fs::remove_file(&prepared.journal_path).await;
}
