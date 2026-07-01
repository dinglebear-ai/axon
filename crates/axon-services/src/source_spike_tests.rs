use super::*;
use axon_source_ledger::{SourceIdentity, SourceKind, SourceLedgerStore};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::path::PathBuf;
use tempfile::TempDir;

async fn test_store() -> Result<SourceLedgerStore, Box<dyn Error + Send + Sync>> {
    Ok(SourceLedgerStore::new(
        axon_jobs::store::open_sqlite_pool(":memory:").await?,
    ))
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn spike_input(root: PathBuf) -> LocalSourceSpikeInput {
    LocalSourceSpikeInput {
        root,
        collection: "axon-test".to_string(),
        owner: "source-spike-test".to_string(),
    }
}

#[tokio::test]
async fn markdown_fixture_creates_uncommitted_local_source_generation()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let temp = TempDir::new()?;
    let root = temp.path().join("docs");
    tokio::fs::create_dir_all(&root).await?;
    let body = "# Ledger Spike\n\nA markdown source document for the local ledger spike.\n";
    tokio::fs::write(root.join("README.md"), body).await?;
    let store = test_store().await?;

    let output = prepare_local_source_spike(spike_input(root.clone()), &store).await?;

    assert_eq!(output.source_kind, SourceKind::LocalCode);
    assert_eq!(output.source_kind.as_str(), "local_code");
    assert_eq!(output.collection, "axon-test");
    assert_eq!(output.generation, 1);
    assert_eq!(output.status.source_id, output.source_id);
    assert_eq!(output.status.source_kind, SourceKind::LocalCode);
    assert_eq!(output.status.committed_generation, 0);
    assert_eq!(output.status.active_generation, Some(output.generation));
    assert_eq!(output.status.cleanup_debt_count, 0);

    assert_eq!(output.manifest_items.len(), 1);
    let item = &output.manifest_items[0];
    assert_eq!(item.item_key, "README.md");
    assert_eq!(item.content_hash, sha256_hex(body));
    assert_eq!(item.size_bytes, body.len() as i64);
    assert!(item.modified_at_ms > 0);

    assert_eq!(output.prepared_docs.len(), 1);
    let prepared = &output.prepared_docs[0];
    assert!(prepared.url().starts_with("file://"));
    assert_eq!(prepared.domain(), "local");
    assert_eq!(prepared.source_type(), "local_code");
    assert_eq!(prepared.content_type(), "text");
    assert_eq!(prepared.title(), Some("README.md"));
    assert!(!prepared.chunks().is_empty());
    let payload = prepared.ledger_payload().expect("ledger payload");
    assert_eq!(payload.source_id(), output.source_id);
    assert_eq!(payload.source_kind(), "local_code");
    assert_eq!(payload.generation(), output.generation);
    assert_eq!(payload.item_key(), "README.md");
    assert_eq!(payload.index_version(), 1);
    let doc_extra = prepared.extra().expect("doc extra");
    assert_eq!(doc_extra["local_file_path"], "README.md");
    assert_eq!(doc_extra["local_file_hash"], sha256_hex(body));
    assert_eq!(doc_extra["local_file_size_bytes"], body.len() as i64);
    assert!(doc_extra["local_file_modified_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(doc_extra["local_content_kind"], "markdown");
    let chunk_extra = prepared.chunk_extra().first().expect("chunk metadata");
    assert_eq!(chunk_extra["chunk_content_kind"], "markdown");
    assert!(
        chunk_extra["chunk_locator"]
            .as_str()
            .unwrap()
            .contains("README.md#L")
    );
    assert!(chunk_extra["source_range"]["byte_start"].as_u64().is_some());
    assert!(chunk_extra["source_range"]["line_start"].as_u64().is_some());

    assert_eq!(output.cleanup_placeholders.len(), 1);
    let placeholder = &output.cleanup_placeholders[0];
    assert_eq!(placeholder.source_id, output.source_id);
    assert_eq!(placeholder.generation, output.generation);
    assert!(!placeholder.executed);

    let status = store.source_status(&output.source_id).await?;
    assert_eq!(status.committed_generation, 0);
    assert_eq!(status.active_generation, Some(output.generation));
    Ok(())
}

#[tokio::test]
async fn rust_fixture_uses_code_chunk_metadata_and_relative_keys()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let temp = TempDir::new()?;
    let root = temp.path().join("repo");
    let src = root.join("src");
    tokio::fs::create_dir_all(&src).await?;
    let body = format!(
        "pub struct Spike;\n\nimpl Spike {{\n    pub fn run(&self) -> usize {{\n{}\n    }}\n}}\n",
        (0..90)
            .map(|i| format!("        let value_{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    tokio::fs::write(src.join("lib.rs"), &body).await?;
    let store = test_store().await?;

    let output = prepare_local_source_spike(spike_input(root), &store).await?;

    assert_eq!(output.source_kind, SourceKind::LocalCode);
    assert_eq!(output.collection, "axon-test");
    assert_eq!(output.status.committed_generation, 0);
    assert_eq!(output.status.active_generation, Some(output.generation));
    assert_eq!(output.manifest_items.len(), 1);
    let item = &output.manifest_items[0];
    assert_eq!(item.item_key, "src/lib.rs");
    assert_eq!(item.content_hash, sha256_hex(&body));
    assert_eq!(item.size_bytes, body.len() as i64);
    assert!(item.modified_at_ms > 0);

    let prepared = output.prepared_docs.first().expect("prepared rust doc");
    assert_eq!(prepared.source_type(), "local_code");
    assert_eq!(prepared.content_type(), "text");
    assert_eq!(prepared.title(), Some("src/lib.rs"));
    assert!(!prepared.chunks().is_empty());
    let payload = prepared.ledger_payload().expect("ledger payload");
    assert_eq!(payload.source_id(), output.source_id);
    assert_eq!(payload.source_kind(), "local_code");
    assert_eq!(payload.generation(), output.generation);
    assert_eq!(payload.item_key(), "src/lib.rs");
    assert_eq!(payload.index_version(), 1);
    let doc_extra = prepared.extra().expect("doc extra");
    assert_eq!(doc_extra["code_file_path"], "src/lib.rs");
    assert_eq!(doc_extra["code_language"], "rust");
    assert_eq!(doc_extra["code_is_test"], false);
    assert_eq!(doc_extra["local_file_path"], "src/lib.rs");
    assert_eq!(doc_extra["local_file_hash"], sha256_hex(&body));
    assert_eq!(doc_extra["local_file_size_bytes"], body.len() as i64);
    assert!(doc_extra["local_file_modified_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(doc_extra["local_content_kind"], "code_or_config");

    let chunk_extra = prepared
        .chunk_extra()
        .iter()
        .find(|extra| extra["chunk_content_kind"] == "code")
        .expect("code chunk metadata");
    assert_eq!(chunk_extra["code_chunk_source"], "tree_sitter");
    assert!(
        chunk_extra["chunk_locator"]
            .as_str()
            .unwrap()
            .contains("src/lib.rs#L")
    );
    assert!(chunk_extra["code_line_start"].as_u64().is_some());
    assert!(chunk_extra["code_line_end"].as_u64().is_some());
    assert!(chunk_extra["source_range"]["line_start"].as_u64().is_some());
    assert!(
        chunk_extra
            .get("symbol_name")
            .and_then(|value| value.as_str())
            .is_some()
    );

    assert_eq!(output.cleanup_placeholders.len(), 1);
    assert_eq!(output.cleanup_placeholders[0].source_id, output.source_id);
    assert_eq!(output.cleanup_placeholders[0].generation, output.generation);
    assert!(!output.cleanup_placeholders[0].executed);
    Ok(())
}

#[tokio::test]
async fn invalid_utf8_releases_lease_without_allocating_generation()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let temp = TempDir::new()?;
    let root = temp.path().join("repo");
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::write(root.join("bad.rs"), [0xff, 0xfe, 0xfd]).await?;
    let canonical_root = tokio::fs::canonicalize(&root).await?;
    let source_id = local_source_id(&canonical_root)?;
    let store = test_store().await?;

    let err = prepare_local_source_spike(spike_input(root), &store)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "expected UTF-8 failure, got: {err}"
    );
    assert_eq!(store.max_generation(&source_id).await?, 0);
    assert_eq!(
        store.source_status(&source_id).await?.active_generation,
        None
    );
    assert!(
        store
            .acquire_lease(
                &SourceIdentity::new(&source_id, SourceKind::LocalCode, "axon-test", 1),
                "next-owner",
                60_000,
            )
            .await?
    );
    Ok(())
}
