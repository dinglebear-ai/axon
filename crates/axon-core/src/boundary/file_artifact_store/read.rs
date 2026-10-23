//! Inline artifact reads have an admission limit; large downloads use streaming.
use std::{io, path::Path};
use tokio::io::AsyncReadExt as _;
pub(super) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub(super) async fn bounded_read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path).await?;
    let metadata = file.metadata().await?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "artifact inline read admission exceeded or file is nonregular; limit={limit} bytes; use /v1/artifacts/<id>/content streaming download or an admitted byte range"
            ),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "artifact grew beyond inline read limit={limit} bytes; retry via streaming artifact download"
            ),
        ));
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn oversized_sparse_file_rejected_before_content_read() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        temp.as_file().set_len(256 * 1024 * 1024).unwrap();
        let error = bounded_read(temp.path(), 32).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("streaming"));
        std::fs::write(temp.path(), b"admitted").unwrap();
        assert_eq!(bounded_read(temp.path(), 32).await.unwrap(), b"admitted");
    }
}
