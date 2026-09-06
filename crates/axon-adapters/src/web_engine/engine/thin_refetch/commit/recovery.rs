use super::*;

pub(super) async fn content_matches(path: &Path, expected_len: u64, expected_hash: &str) -> bool {
    let Ok(bytes) = tokio::fs::read(path).await else {
        return false;
    };
    bytes.len() as u64 == expected_len && hex::encode(Sha256::digest(&bytes)) == expected_hash
}

pub(super) async fn rollback_manifest(
    output_dir: &Path,
    journal: &RefetchCommitJournal,
) -> std::io::Result<()> {
    let manifest_path = output_dir.join("manifest.jsonl");
    let manifest = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&manifest_path)
        .await?;
    let current_len = manifest.metadata().await?.len();
    if current_len < journal.manifest_start {
        return Ok(());
    }
    let owned_len = journal.manifest_line_len.unwrap_or(0);
    let expected_end = journal.manifest_start.saturating_add(owned_len);
    if current_len < expected_end {
        return Err(std::io::Error::other(
            "manifest ends inside the recorded transaction",
        ));
    }
    if let Some(expected_hash) = journal.manifest_line_hash.as_deref() {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut readable = tokio::fs::File::open(&manifest_path).await?;
        readable
            .seek(std::io::SeekFrom::Start(journal.manifest_start))
            .await?;
        let mut owned_line = vec![0_u8; owned_len as usize];
        readable.read_exact(&mut owned_line).await?;
        if hex::encode(Sha256::digest(&owned_line)) != expected_hash {
            return Err(std::io::Error::other(
                "manifest suffix does not belong to this transaction",
            ));
        }
    }
    manifest.set_len(journal.manifest_start).await?;
    manifest.sync_all().await
}
