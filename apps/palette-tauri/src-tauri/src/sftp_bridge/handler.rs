//! `russh::client::Handler` implementation wiring host-key verification to
//! `sftp_known_hosts::evaluate_host_key`.
//!
//! This is the ONLY thing standing between the SFTP feature and a silent
//! MITM on every connection — see `sftp_known_hosts.rs`'s module doc for why
//! that module's regression test is a hard merge-blocker. `check_server_key`
//! below must never return `Ok(true)` for anything other than an already
//! `TrustedMatch`ed key or a new host the frontend just had the user confirm
//! (`trust_new_host`, set only after a round-trip trust prompt — see
//! `commands::sftp_connect`).

use std::sync::{Arc, Mutex};

use russh::keys::PublicKey;

use crate::sftp_known_hosts::{
    HostKeyDecision, KnownHostEntry, KnownHostsStore, evaluate_host_key,
};

/// Outcome of a connection attempt's host-key check, surfaced up to
/// `sftp_connect` so it can either proceed (already trusted or freshly
/// confirmed — `entry` is the exact key/fingerprint the server presented,
/// ready to pin if this was a first-trust confirmation), ask the frontend to
/// prompt the user (new, unconfirmed host), or hard-fail (a pinned
/// fingerprint changed).
#[derive(Debug, Clone)]
pub(crate) enum HandshakeOutcome {
    Proceeded {
        entry: KnownHostEntry,
    },
    NeedsTrustPrompt {
        entry: KnownHostEntry,
    },
    Mismatch {
        pinned_fingerprint: String,
        seen_fingerprint: String,
    },
}

/// Implements `russh::client::Handler` for one connection attempt.
///
/// `check_server_key` only receives the presented key, not the peer address,
/// so `host`/`port` are threaded in from the caller. `outcome` is written
/// once during the handshake and read back by `sftp_connect` afterward —
/// `russh::client::connect` gives no other channel back to the caller for a
/// per-key decision this rich (accept / needs-prompt / mismatch).
pub(crate) struct SftpClientHandler {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) known_hosts: KnownHostsStore,
    /// Set only when the frontend already showed the trust prompt for this
    /// exact host/port and the user confirmed — see `sftp_connect`'s
    /// `trust_new_host` parameter.
    pub(crate) trust_new_host: bool,
    pub(crate) outcome: Arc<Mutex<Option<HandshakeOutcome>>>,
}

impl russh::client::Handler for SftpClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let key_type = server_public_key.algorithm().to_string();
        let fingerprint = server_public_key
            .fingerprint(Default::default())
            .to_string();
        // The presented key's identity, independent of the trust decision
        // below — used both to build a NewHostNeedsPrompt/Proceeded entry and
        // (on trust_new_host) as exactly what gets pinned to disk.
        let presented_entry = KnownHostEntry {
            host: self.host.clone(),
            port: self.port,
            key_type: key_type.clone(),
            fingerprint: fingerprint.clone(),
            first_seen_unix: 0,
        };
        let decision = evaluate_host_key(
            &self.known_hosts,
            &self.host,
            self.port,
            &key_type,
            &fingerprint,
        );

        let (accept, outcome) = match decision {
            HostKeyDecision::TrustedMatch => (
                true,
                HandshakeOutcome::Proceeded {
                    entry: presented_entry,
                },
            ),
            HostKeyDecision::NewHostNeedsPrompt(entry) => {
                if self.trust_new_host {
                    (true, HandshakeOutcome::Proceeded { entry })
                } else {
                    (false, HandshakeOutcome::NeedsTrustPrompt { entry })
                }
            }
            HostKeyDecision::Mismatch {
                pinned,
                seen_fingerprint,
            } => (
                false,
                HandshakeOutcome::Mismatch {
                    pinned_fingerprint: pinned.fingerprint,
                    seen_fingerprint,
                },
            ),
        };

        if let Ok(mut guard) = self.outcome.lock() {
            *guard = Some(outcome);
        }
        Ok(accept)
    }
}
