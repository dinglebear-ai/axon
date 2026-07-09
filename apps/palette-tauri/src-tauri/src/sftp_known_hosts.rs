//! TOFU (trust-on-first-use) host-key fingerprint store for the SFTP bridge,
//! backing `russh::client::Handler::check_server_key`.
//!
//! Persisted to `<app_config_dir>/sftp_known_hosts.json`, alongside
//! `settings.json`, via the same `atomic_write` helper `files_bridge.rs`
//! uses. This is a dedicated file rather than a `PaletteSettings` field
//! because it is an append/revoke-oriented trust ledger, not a preference —
//! keeping it separate also means a corrupt/truncated settings.json write
//! can never take the host-key trust store down with it.
//!
//! # Why TOFU, and why this is a hard merge-blocker
//!
//! `check_server_key` is the ONLY thing standing between this feature and a
//! silent MITM on every SFTP connection. A stubbed `Ok(true)`-always
//! implementation would pass every other test in this plan, since none of
//! them exercise the callback — see
//! `evaluate_host_key_flags_a_new_host_for_trust_prompt_not_auto_accept`
//! (sftp_known_hosts_tests.rs), which fails specifically if a new host ever
//! resolves to `TrustedMatch` instead of requiring an explicit trust
//! decision.
//!
//! TOFU matches OpenSSH's own default (`~/.ssh/known_hosts` behavior) — a
//! reasonable fit for this tool's threat model (a developer-facing palette,
//! not a hardened enterprise SSH client). A pinned fingerprint that later
//! mismatches on reconnect is a hard failure, never silently re-pinned,
//! matching how OpenSSH refuses to connect and requires explicit
//! `known_hosts` editing after a real host-key change.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::persistence::atomic_write;

const KNOWN_HOSTS_FILE: &str = "sftp_known_hosts.json";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub first_seen_unix: u64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct KnownHostsStore(pub(crate) Vec<KnownHostEntry>);

#[derive(Debug)]
pub(crate) enum HostKeyDecision {
    TrustedMatch,
    NewHostNeedsPrompt(KnownHostEntry),
    Mismatch {
        pinned: KnownHostEntry,
        seen_fingerprint: String,
    },
}

/// Normalizes a hostname for comparison/storage: lowercased, trailing dot
/// stripped. Matches OpenSSH's own `known_hosts` case-insensitivity for DNS
/// names so `example.com`, `EXAMPLE.COM`, and `example.com.` all pin to the
/// same entry instead of each triggering their own "new host" TOFU prompt.
///
/// Deliberately does NOT attempt to unify an IP address with a hostname that
/// happens to resolve to it — those remain intentionally distinct identities,
/// matching OpenSSH's `known_hosts` behavior (a pinned hostname entry doesn't
/// cover connecting by raw IP, and vice versa).
fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Evaluate a server-presented host key against the pinned store. Never
/// mutates `store` — pinning only happens via an explicit `pin_host_key`
/// call after the frontend confirms a `NewHostNeedsPrompt` decision.
pub(crate) fn evaluate_host_key(
    store: &KnownHostsStore,
    host: &str,
    port: u16,
    key_type: &str,
    fingerprint: &str,
) -> HostKeyDecision {
    let normalized_host = normalize_host(host);
    match store
        .0
        .iter()
        .find(|entry| normalize_host(&entry.host) == normalized_host && entry.port == port)
    {
        Some(entry) if entry.fingerprint == fingerprint => HostKeyDecision::TrustedMatch,
        Some(entry) => HostKeyDecision::Mismatch {
            pinned: entry.clone(),
            seen_fingerprint: fingerprint.to_string(),
        },
        None => HostKeyDecision::NewHostNeedsPrompt(KnownHostEntry {
            host: normalized_host,
            port,
            key_type: key_type.to_string(),
            fingerprint: fingerprint.to_string(),
            first_seen_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }),
    }
}

pub(crate) fn pin_host_key(store: &mut KnownHostsStore, entry: KnownHostEntry) {
    let normalized_host = normalize_host(&entry.host);
    store.0.retain(|existing| {
        !(normalize_host(&existing.host) == normalized_host && existing.port == entry.port)
    });
    store.0.push(entry);
}

pub(crate) fn revoke_host_key(store: &mut KnownHostsStore, host: &str, port: u16) {
    let normalized_host = normalize_host(host);
    store
        .0
        .retain(|entry| !(normalize_host(&entry.host) == normalized_host && entry.port == port));
}

fn known_hosts_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(KNOWN_HOSTS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

pub(crate) fn load_known_hosts(app: &AppHandle) -> Result<KnownHostsStore, String> {
    let path = known_hosts_path(app)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|err| format!("failed to parse {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(KnownHostsStore::default()),
        Err(err) => Err(format!("failed to read {}: {err}", path.display())),
    }
}

pub(crate) fn save_known_hosts(app: &AppHandle, store: &KnownHostsStore) -> Result<(), String> {
    let path = known_hosts_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|err| err.to_string())?;
    atomic_write(&path, json.as_bytes()).map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "sftp_known_hosts_tests.rs"]
mod tests;
