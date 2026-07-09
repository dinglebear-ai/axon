//! SSH/SFTP client bridge for the Files view's remote-browsing feature.
//!
//! # Registration gate (now lifted)
//!
//! `check_server_key` (see `handler::SftpClientHandler`) calls
//! `sftp_known_hosts::evaluate_host_key` and only accepts a `TrustedMatch` or
//! a frontend-confirmed first-trust — every other outcome is rejected. The
//! four SFTP commands plus the two known-hosts commands are registered in
//! `lib.rs`'s `invoke_handler!` alongside this gate landing, not before —
//! see this module's git history for the earlier commit that added this
//! bridge without registering any command, specifically to make it
//! impossible to ship a live, callable SFTP surface with an unconditionally
//! accepting host-key check.
//!
//! # Threat model
//!
//! Unlike `files_bridge.rs`'s local root-escape checks, there is no local
//! canonicalization concept for a remote server's filesystem — the SFTP
//! server itself is the authority on what paths exist and are reachable.
//! What SFTP *does* need, that local files don't, is host-key verification
//! (MITM protection — see `sftp_known_hosts.rs`) and credential handling
//! (`private_key_path`, which — unlike remote paths — IS a local file and
//! DOES need the same local-path rigor as `files_bridge.rs`; see
//! `validate_private_key_path` below).

use std::{collections::HashMap, path::Path};

use tokio::sync::Mutex;

pub(crate) mod commands;
mod handler;

pub(crate) type ConnectionId = String;

/// Live SFTP sessions keyed by connection id, held as Tauri managed state.
///
/// `tokio::sync::Mutex`, not `std::sync::Mutex`: SFTP list/read are async
/// network round-trips, and holding a `std::sync::Mutex` guard across an
/// `.await` blocks the async executor thread for the call's full latency —
/// exactly what clippy's `await_holding_lock` lint exists to catch.
pub(crate) struct SftpConnections(
    pub(crate) Mutex<HashMap<ConnectionId, russh_sftp::client::SftpSession>>,
);

impl SftpConnections {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Closes every live SFTP session and clears the map. Called from the
    /// `RunEvent::Exit` handler in `lib.rs::run` so sessions don't leak past
    /// process exit — `sftp_disconnect` only ever closes one connection at a
    /// time and there is no other app-exit hook that reaches this state.
    pub(crate) async fn close_all(&self) {
        let mut guard = self.0.lock().await;
        for (_, sftp) in guard.drain() {
            let _ = sftp.close().await;
        }
    }
}

/// A `uuid::Uuid::new_v4()`-backed connection id, not a monotonic counter —
/// sequential/guessable ids are a needless weakness even under this app's
/// "renderer is trusted" threat model.
pub(crate) fn new_connection_id() -> ConnectionId {
    uuid::Uuid::new_v4().to_string()
}

/// Normalizes a renderer-supplied remote path string. No canonicalization
/// against a local root (there is none for a remote filesystem) — this only
/// rejects NUL bytes, matching the minimal safety `files_bridge.rs` applies
/// before even touching a path.
pub(crate) fn normalize_remote_path(path: &str) -> Result<String, String> {
    if path.contains('\0') {
        return Err("path must not contain NUL bytes".to_string());
    }
    Ok(path.to_string())
}

/// Local-path rigor for `private_key_path` — this field is NOT a remote
/// path, it's a local file used to authenticate, so it gets the same
/// canonicalize/symlink/regular-file checks `files_bridge.rs` applies to its
/// local root, unlike the rest of the SFTP surface which correctly has no
/// local-root concept.
pub(crate) fn validate_private_key_path(path: &Path) -> Result<std::path::PathBuf, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|err| format!("private key path {} is invalid: {err}", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical).map_err(|err| err.to_string())?;
    if metadata.is_symlink() {
        return Err("private key path must not be a symlink".to_string());
    }
    if !metadata.is_file() {
        return Err("private key path must be a regular file, not a directory".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            crate::diag::warn(&format!(
                "private key {} is group/world-readable (mode {mode:o}); \
                 consider `chmod 600` to match SSH client conventions",
                canonical.display()
            ));
        }
    }
    Ok(canonical)
}

#[cfg(test)]
#[path = "sftp_bridge_tests.rs"]
mod tests;
