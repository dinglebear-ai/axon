//! The four SFTP `#[tauri::command]`s: connect (with TOFU host-key gating),
//! list a directory, read a file, and disconnect — plus the two known-hosts
//! management commands. See `sftp_bridge.rs`'s module doc for the
//! registration gate: these are only wired into `lib.rs`'s
//! `invoke_handler!` once this module exists and its handshake goes through
//! `handler::SftpClientHandler`.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::{
    ConnectionId, SftpConnections, new_connection_id, normalize_remote_path,
    validate_private_key_path,
};
use crate::sftp_bridge::handler::{HandshakeOutcome, SftpClientHandler};
use crate::sftp_known_hosts::{
    KnownHostEntry, load_known_hosts, pin_host_key, revoke_host_key, save_known_hosts,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpConnectionInput {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: String,
    /// Set by the frontend on a re-connect attempt after the user confirmed
    /// `SftpTrustPrompt` for this exact host/port/fingerprint.
    #[serde(default)]
    pub trust_new_host: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum SftpConnectResult {
    Connected { connection_id: ConnectionId },
    PendingTrust { entry: KnownHostEntry },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_unix: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpDirListing {
    pub path: String,
    pub entries: Vec<SftpEntry>,
    /// True when the remote directory had more entries than
    /// `MAX_SFTP_DIR_ENTRIES` and the list was truncated. The local
    /// `files_list_dir` has no such cap (it reads from a trusted local
    /// filesystem); this one bounds an unbounded remote fetch — a first pass,
    /// not pagination.
    pub truncated: bool,
}

/// Cap on entries returned by `sftp_list_dir` for a single directory. Chosen
/// as a reasonable first-pass ceiling (no local `files_list_dir` precedent to
/// match, since local listings aren't capped) — revisit with real pagination
/// if remote directories routinely exceed this.
const MAX_SFTP_DIR_ENTRIES: usize = 2000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpFileContents {
    pub path: String,
    pub content: String,
}

/// SFTP file previews are read over the network (unlike local disk reads),
/// so latency scales with both file size and round-trip time. This reuses
/// `files_bridge::MAX_TEXT_FILE_BYTES`'s 5 MiB ceiling as a first pass —
/// revisit with a smaller cap if remote previews prove too slow in practice
/// (tracked as an open question, not a hard requirement for v1).
const MAX_SFTP_TEXT_FILE_BYTES: usize = 5 * 1024 * 1024;

/// Bound on each phase of `sftp_connect` (TCP connect + SSH handshake, then
/// publickey auth, then channel-open/SFTP-subsystem-negotiate) — a slow or
/// unresponsive host would otherwise hang the command indefinitely with no
/// way for the UI to cancel it.
const SFTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Outcome of the handshake phase: either the fully-established SFTP session
/// (auth done, channel open), or an early return the caller should surface
/// as-is (a pending trust prompt or a rejected/mismatched host key).
enum HandshakePhaseResult {
    Session(russh_sftp::client::SftpSession),
    Early(SftpConnectResult),
}

/// Runs the TCP-connect+SSH-handshake, then (unless the handshake short-
/// circuits into a trust prompt or mismatch) publickey auth and SFTP
/// channel-open — each phase individually bounded by `SFTP_CONNECT_TIMEOUT`.
/// Split out of `sftp_connect` to keep that command under the monolith
/// policy's per-function line cap; behavior is unchanged.
async fn establish_sftp_session(
    app: &AppHandle,
    key_path: &std::path::Path,
    profile: &SftpConnectionInput,
) -> Result<HandshakePhaseResult, String> {
    let known_hosts = load_known_hosts(app)?;

    let outcome = Arc::new(Mutex::new(None));
    let handler = SftpClientHandler {
        host: profile.host.clone(),
        port: profile.port,
        known_hosts,
        trust_new_host: profile.trust_new_host,
        outcome: outcome.clone(),
    };

    let config = Arc::new(russh::client::Config::default());
    // The whole handshake+auth+channel-open sequence is bounded by a single
    // timeout per phase — without this, a slow or unresponsive host (firewall
    // black-holing the TCP connect, or a server that accepts the connection
    // but never completes the SSH handshake) hangs this command forever with
    // no way for the UI to cancel it.
    let connect_result = tokio::time::timeout(
        SFTP_CONNECT_TIMEOUT,
        russh::client::connect(config, (profile.host.as_str(), profile.port), handler),
    )
    .await
    .map_err(|_| {
        format!(
            "connecting to {}:{} timed out after {}s",
            profile.host,
            profile.port,
            SFTP_CONNECT_TIMEOUT.as_secs()
        )
    })?;

    let handshake_outcome = outcome.lock().ok().and_then(|mut g| g.take());
    let proceeded_entry = match handshake_outcome {
        Some(HandshakeOutcome::NeedsTrustPrompt { entry }) => {
            return Ok(HandshakePhaseResult::Early(
                SftpConnectResult::PendingTrust { entry },
            ));
        }
        Some(HandshakeOutcome::Mismatch {
            pinned_fingerprint,
            seen_fingerprint,
        }) => {
            return Err(format!(
                "host key for {}:{} changed (expected {pinned_fingerprint}, got {seen_fingerprint}) — \
                 refusing to connect. If this change is expected, revoke the old pinned key first.",
                profile.host, profile.port
            ));
        }
        Some(HandshakeOutcome::Proceeded { entry }) => Some(entry),
        None => None,
    };

    let mut session = connect_result.map_err(|err| err.to_string())?;

    // The handshake succeeded and check_server_key accepted the key. If this
    // was a first-trust confirmation (trust_new_host), persist the pin now
    // that the connection is actually proven live, not merely offered — the
    // exact entry the handler observed during the handshake (host/port/
    // key_type/fingerprint) is reused verbatim, no re-derivation needed.
    if profile.trust_new_host
        && let Some(mut entry) = proceeded_entry
    {
        entry.first_seen_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut store = load_known_hosts(app)?;
        pin_host_key(&mut store, entry);
        save_known_hosts(app, &store)?;
    }

    let key = russh::keys::load_secret_key(key_path, None).map_err(|err| err.to_string())?;
    let key = Arc::new(key);
    let auth = tokio::time::timeout(
        SFTP_CONNECT_TIMEOUT,
        session.authenticate_publickey(
            profile.username.clone(),
            russh::keys::PrivateKeyWithHashAlg::new(key, Some(russh::keys::HashAlg::Sha256)),
        ),
    )
    .await
    .map_err(|_| {
        format!(
            "authenticating to {}:{} timed out after {}s",
            profile.host,
            profile.port,
            SFTP_CONNECT_TIMEOUT.as_secs()
        )
    })?
    .map_err(|err| err.to_string())?;
    if !auth.success() {
        return Err(format!(
            "authentication failed for {}@{}:{}",
            profile.username, profile.host, profile.port
        ));
    }

    let sftp = tokio::time::timeout(SFTP_CONNECT_TIMEOUT, async {
        let channel = session
            .channel_open_session()
            .await
            .map_err(|err| err.to_string())?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|err| err.to_string())?;
        let stream = channel.into_stream();
        russh_sftp::client::SftpSession::new(stream)
            .await
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|_| {
        format!(
            "opening the SFTP channel to {}:{} timed out after {}s",
            profile.host,
            profile.port,
            SFTP_CONNECT_TIMEOUT.as_secs()
        )
    })??;

    Ok(HandshakePhaseResult::Session(sftp))
}

#[tauri::command]
pub(crate) async fn sftp_connect(
    app: AppHandle,
    connections: tauri::State<'_, SftpConnections>,
    profile: SftpConnectionInput,
) -> Result<SftpConnectResult, String> {
    // v1 is single-active-connection by design (see sftp_bridge.rs's module
    // doc): reject a new connect attempt outright while one is already open,
    // rather than silently accumulating sessions the frontend never surfaces
    // and never cleans up individually. The frontend must call
    // `sftp_disconnect` first (as `connectSftp` in FilesView.tsx already does
    // before dispatching a new connect).
    if !connections.0.lock().await.is_empty() {
        return Err(
            "an SFTP connection is already open — disconnect it before connecting to a new host"
                .to_string(),
        );
    }
    let key_path = validate_private_key_path(std::path::Path::new(&profile.private_key_path))?;

    let sftp = match establish_sftp_session(&app, &key_path, &profile).await? {
        HandshakePhaseResult::Early(result) => return Ok(result),
        HandshakePhaseResult::Session(sftp) => sftp,
    };

    let connection_id = new_connection_id();
    connections
        .0
        .lock()
        .await
        .insert(connection_id.clone(), sftp);
    Ok(SftpConnectResult::Connected { connection_id })
}

#[tauri::command]
pub(crate) async fn sftp_list_dir(
    connections: tauri::State<'_, SftpConnections>,
    connection_id: String,
    path: Option<String>,
) -> Result<SftpDirListing, String> {
    let target = path.unwrap_or_default();
    let normalized = normalize_remote_path(&target)?;
    let guard = connections.0.lock().await;
    let sftp = guard
        .get(&connection_id)
        .ok_or_else(|| "SFTP connection not found".to_string())?;
    let list_path = if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized.clone()
    };
    let read_dir = sftp
        .read_dir(&list_path)
        .await
        .map_err(|err| err.to_string())?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in read_dir {
        if entries.len() >= MAX_SFTP_DIR_ENTRIES {
            truncated = true;
            break;
        }
        let metadata = entry.metadata();
        entries.push(SftpEntry {
            name: entry.file_name(),
            path: entry.path(),
            is_dir: entry.file_type().is_dir(),
            size: metadata.size.unwrap_or(0),
            modified_unix: metadata.mtime.map(u64::from),
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(SftpDirListing {
        path: normalized,
        entries,
        truncated,
    })
}

#[tauri::command]
pub(crate) async fn sftp_read_file(
    connections: tauri::State<'_, SftpConnections>,
    connection_id: String,
    path: String,
) -> Result<SftpFileContents, String> {
    let normalized = normalize_remote_path(&path)?;
    let guard = connections.0.lock().await;
    let sftp = guard
        .get(&connection_id)
        .ok_or_else(|| "SFTP connection not found".to_string())?;
    // Stat before reading: reject oversized files based on the server-reported
    // size instead of buffering the whole file into memory first and checking
    // afterward. Mirrors `files_bridge::files_read_file`'s `metadata.len()`
    // check before `fs::read`. A server that lies about its own file size
    // (reports small, serves large) is not defended against here — that would
    // need a streaming/bounded read, out of scope for this pass.
    let metadata = sftp
        .metadata(&normalized)
        .await
        .map_err(|err| err.to_string())?;
    if let Some(size) = metadata.size
        && size > MAX_SFTP_TEXT_FILE_BYTES as u64
    {
        return Err(format!(
            "file is too large to preview ({size} bytes, limit {MAX_SFTP_TEXT_FILE_BYTES})"
        ));
    }
    let bytes = sftp
        .read(&normalized)
        .await
        .map_err(|err| err.to_string())?;
    if bytes.len() > MAX_SFTP_TEXT_FILE_BYTES {
        return Err(format!(
            "file is too large to preview ({} bytes, limit {MAX_SFTP_TEXT_FILE_BYTES})",
            bytes.len()
        ));
    }
    let content =
        String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".to_string())?;
    Ok(SftpFileContents {
        path: normalized,
        content,
    })
}

#[tauri::command]
pub(crate) async fn sftp_disconnect(
    connections: tauri::State<'_, SftpConnections>,
    connection_id: String,
) -> Result<(), String> {
    let mut guard = connections.0.lock().await;
    if let Some(sftp) = guard.remove(&connection_id) {
        let _ = sftp.close().await;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn sftp_list_known_hosts(app: AppHandle) -> Result<Vec<KnownHostEntry>, String> {
    Ok(load_known_hosts(&app)?.0)
}

#[tauri::command]
pub(crate) fn sftp_revoke_known_host(
    app: AppHandle,
    host: String,
    port: u16,
) -> Result<(), String> {
    let mut store = load_known_hosts(&app)?;
    revoke_host_key(&mut store, &host, port);
    save_known_hosts(&app, &store)
}
