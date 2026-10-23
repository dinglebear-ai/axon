//! Seek before reading, so a small range never allocates the whole artifact.
use super::*;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

pub(super) async fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let file = open(path).await?;
    if file.metadata().await?.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "artifact read exceeds {limit} byte admission; use streaming artifact download"
            ),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "artifact grew beyond admitted read window",
        ));
    }
    Ok(bytes)
}
async fn open(path: &Path) -> std::io::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path).await?;
    if !file.metadata().await?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "artifact must be a regular file",
        ));
    }
    Ok(file)
}
pub(super) async fn read_range(path: &Path, range: Option<&str>) -> Result<Vec<u8>, ApiError> {
    let mut file = open(path)
        .await
        .map_err(|error| io_error("artifact.read_failed", path, error))?;
    let length = file
        .metadata()
        .await
        .map_err(|error| io_error("artifact.read_failed", path, error))?
        .len();
    let (start, end) = range
        .map(|range| byte_range(length, range))
        .transpose()?
        .unwrap_or((0, length));
    let count = end - start;
    if count > axon_api::source::MAX_UPLOAD_BYTES {
        return Err(artifact_error(
            "artifact.inline_byte_limit",
            format!(
                "requested artifact read is {count} bytes; inline limit={} bytes; request a smaller byte range or use /v1/artifacts/<id>/content streaming download",
                axon_api::source::MAX_UPLOAD_BYTES
            ),
        ));
    }
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|error| io_error("artifact.read_failed", path, error))?;
    let mut bytes = Vec::new();
    file.take(count)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| io_error("artifact.read_failed", path, error))?;
    if bytes.len() as u64 != count {
        return Err(artifact_error(
            "artifact.read_changed",
            "artifact changed during range read; inspect current artifact and retry",
        ));
    }
    Ok(bytes)
}
fn byte_range(length: u64, raw: &str) -> Result<(u64, u64), ApiError> {
    let range = raw.trim().strip_prefix("bytes=").unwrap_or(raw.trim());
    if range.contains(',') {
        return Err(artifact_error(
            "artifact.invalid_range",
            "multiple artifact byte ranges are not supported",
        ));
    }
    let Some((start, end)) = range.split_once('-') else {
        return Err(artifact_error(
            "artifact.invalid_range",
            "byte range must use START-END, START-, or -SUFFIX_LENGTH",
        ));
    };
    if length == 0 {
        return Err(artifact_error(
            "artifact.range_not_satisfiable",
            "byte range cannot be satisfied for empty content",
        ));
    }
    let (start, end_exclusive) = if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| {
            artifact_error(
                "artifact.invalid_range",
                "byte range suffix length must be an integer",
            )
        })?;
        if suffix == 0 {
            return Err(artifact_error(
                "artifact.invalid_range",
                "byte range suffix length must be greater than zero",
            ));
        }
        (length.saturating_sub(suffix), length)
    } else {
        let start = start.parse::<u64>().map_err(|_| {
            artifact_error(
                "artifact.invalid_range",
                "byte range start must be an integer",
            )
        })?;
        if start >= length {
            return Err(artifact_error(
                "artifact.range_not_satisfiable",
                "byte range starts beyond the end of the artifact",
            ));
        }
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse::<u64>().map_err(|_| {
                artifact_error(
                    "artifact.invalid_range",
                    "byte range end must be an integer",
                )
            })?
        };
        if end < start {
            return Err(artifact_error(
                "artifact.invalid_range",
                "byte range end must not precede its start",
            ));
        }
        (start, end.saturating_add(1).min(length))
    };
    Ok((start, end_exclusive))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_boundaries_reject_invalid_input_without_overflow() {
        assert_eq!(
            byte_range(10, "bytes=8-18446744073709551615").unwrap(),
            (8, 10)
        );
        assert_eq!(
            byte_range(10, "bytes=-18446744073709551615").unwrap(),
            (0, 10)
        );
        assert_eq!(byte_range(10, "bytes=8-").unwrap(), (8, 10));
        for raw in [
            "bytes=10-",
            "bytes=9-8",
            "bytes=-0",
            "bytes=0-1,3-4",
            "bytes=no",
        ] {
            assert!(byte_range(10, raw).is_err(), "accepted invalid range {raw}");
        }
        assert!(byte_range(0, "bytes=0-").is_err());
    }
    #[tokio::test]
    async fn small_range_of_large_sparse_artifact_reads_only_requested_window() {
        use std::io::{Seek as _, Write as _};
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        temp.as_file().set_len(256 * 1024 * 1024).unwrap();
        temp.seek(std::io::SeekFrom::Start(200 * 1024 * 1024))
            .unwrap();
        temp.write_all(b"evidence").unwrap();
        assert_eq!(
            read_range(temp.path(), Some("bytes=209715200-209715207"))
                .await
                .unwrap(),
            b"evidence"
        );
        let error = read_range(temp.path(), None).await.unwrap_err();
        assert_eq!(error.code.0, "artifact.inline_byte_limit");
        assert!(error.message.contains("streaming"));
    }
}
