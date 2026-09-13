use std::future::Future;

use russh_sftp::client::RawSftpSession;
use russh_sftp::client::error::Error;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};

use super::{SFTP_READ_CHUNK_BYTES, SftpEntry, push_entry_bounded, validate_sftp_filename};

pub(super) async fn read_file_bounded(
    raw: &RawSftpSession,
    path: &str,
    limit: usize,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let handle = tokio::time::timeout_at(
        deadline,
        raw.open(path, OpenFlags::READ, FileAttributes::default()),
    )
    .await
    .map_err(|_| "SFTP file read timed out".to_string())?
    .map_err(|err| err.to_string())?
    .handle;
    let read_handle = handle.clone();
    let close_handle = handle.clone();
    read_packets_bounded(
        limit,
        deadline,
        |offset, request_len| {
            let handle = read_handle.clone();
            async move {
                match raw.read(handle, offset, request_len).await {
                    Ok(data) if data.data.is_empty() => Ok(None),
                    Ok(data) => Ok(Some(data.data)),
                    Err(Error::Status(status)) if status.status_code == StatusCode::Eof => Ok(None),
                    Err(err) => Err(err.to_string()),
                }
            }
        },
        || async {
            let _ = raw.close(close_handle).await;
        },
    )
    .await
}

pub(super) async fn read_packets_bounded<Read, ReadFuture, Close, CloseFuture>(
    limit: usize,
    deadline: tokio::time::Instant,
    mut read: Read,
    close: Close,
) -> Result<Vec<u8>, String>
where
    Read: FnMut(u64, u32) -> ReadFuture,
    ReadFuture: Future<Output = Result<Option<Vec<u8>>, String>>,
    Close: FnOnce() -> CloseFuture,
    CloseFuture: Future<Output = ()>,
{
    let mut bytes = Vec::with_capacity(limit.min(SFTP_READ_CHUNK_BYTES as usize));
    let result = loop {
        let remaining = limit.saturating_sub(bytes.len());
        let request_len =
            SFTP_READ_CHUNK_BYTES.min(u32::try_from(remaining + 1).unwrap_or(u32::MAX));
        match tokio::time::timeout_at(deadline, read(bytes.len() as u64, request_len)).await {
            Err(_) => break Err("SFTP file read timed out".to_string()),
            Ok(Ok(None)) => break Ok(bytes),
            Ok(Ok(Some(data))) => {
                if !append_read_packet(&mut bytes, &data, limit) {
                    break Err(format!("file is too large to preview (limit {limit})"));
                }
            }
            Ok(Err(err)) => break Err(err.to_string()),
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), close()).await;
    result
}

pub(super) async fn read_dir_bounded(
    raw: &RawSftpSession,
    path: &str,
    limit: usize,
    timeout: std::time::Duration,
) -> Result<(Vec<SftpEntry>, bool), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let handle = tokio::time::timeout_at(deadline, raw.opendir(path))
        .await
        .map_err(|_| "SFTP directory listing timed out".to_string())?
        .map_err(|err| err.to_string())?
        .handle;
    let mut entries = Vec::with_capacity(limit.min(256));
    let result = 'read: loop {
        match tokio::time::timeout_at(deadline, raw.readdir(handle.clone())).await {
            Err(_) => break Err("SFTP directory listing timed out".to_string()),
            Ok(result) => match result {
                Ok(names) => {
                    for file in names.files {
                        if file.filename == "." || file.filename == ".." {
                            continue;
                        }
                        if let Err(error) = validate_sftp_filename(&file.filename) {
                            break 'read Err(error);
                        }
                        let attrs = file.attrs;
                        let entry_path = if path.ends_with('/') {
                            format!("{path}{}", file.filename)
                        } else {
                            format!("{path}/{}", file.filename)
                        };
                        let entry = SftpEntry {
                            name: file.filename,
                            path: entry_path,
                            is_dir: attrs.file_type().is_dir(),
                            size: attrs.size.unwrap_or(0),
                            modified_unix: attrs.mtime.map(u64::from),
                        };
                        if push_entry_bounded(&mut entries, entry, limit) {
                            break 'read Ok((entries, true));
                        }
                    }
                }
                Err(Error::Status(status)) if status.status_code == StatusCode::Eof => {
                    break Ok((entries, false));
                }
                Err(err) => break Err(err.to_string()),
            },
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), raw.close(handle)).await;
    result
}

pub(super) fn append_read_packet(bytes: &mut Vec<u8>, packet: &[u8], limit: usize) -> bool {
    if packet.len() > limit.saturating_sub(bytes.len()) {
        false
    } else {
        bytes.extend_from_slice(packet);
        true
    }
}
