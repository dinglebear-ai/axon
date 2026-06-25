//! Release-artifact integrity verification for the self-updater.
//!
//! Two independent checks:
//! - **SHA256** (`parse_sha256_sidecar` + `verify_sha256_file`): always enforced;
//!   shares a trust root with the binary (both from the same release).
//! - **Signature** (`resolve_optional_signature` + `verify_optional_signature`,
//!   OPS-H3): an optional detached minisign signature that, once releases are
//!   signed and `AXON_UPDATE_MINISIGN_PUBKEY` is provisioned, gives an
//!   independent trust root. Inert until both are present.

use super::{GithubRelease, ReleaseAssetNames, UpdateOptions, download_to_file, err};
use axon_core::http::http_client;
use sha2::{Digest, Sha256};
use std::env;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::Path;

/// OPS-H3 (bounded): env var holding the minisign public key used to verify the
/// optional `.minisig` release signature. Signature verification is INERT until
/// (a) releases are signed (see `.github/workflows/release.yml`) and (b) this
/// public key is provisioned. When set AND a signature is available, the
/// updater enforces the signature on top of the SHA256 check, giving an
/// independent trust root. MANUAL FOLLOW-UP: distribute/embed the public key.
pub(super) const UPDATE_MINISIGN_PUBKEY: &str = "AXON_UPDATE_MINISIGN_PUBKEY";

pub(super) fn parse_sha256_sidecar(body: &str) -> Result<String, Box<dyn Error>> {
    let hash = body
        .split_whitespace()
        .next()
        .ok_or_else(|| err("empty sha256 sidecar"))?;
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err(format!("invalid sha256 sidecar hash: {hash}")));
    }
    Ok(hash.to_ascii_lowercase())
}

#[cfg(test)]
pub(super) fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), Box<dyn Error>> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected.to_ascii_lowercase() {
        return Err(err(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

pub(super) fn verify_sha256_file(path: &Path, expected: &str) -> Result<(), Box<dyn Error>> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != expected.to_ascii_lowercase() {
        return Err(err(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

/// Resolve the optional detached signature (OPS-H3) into `dest`, returning
/// whether one was found. Best-effort: a missing signature is NOT an error
/// (releases are unsigned until signing is provisioned).
pub(super) async fn resolve_optional_signature(
    options: &UpdateOptions,
    names: &ReleaseAssetNames,
    dest: &Path,
) -> Result<bool, Box<dyn Error>> {
    // Skip resolution entirely when verification is disabled (no public key).
    // This keeps the default (unsigned) update path to a single API round-trip
    // and never fails on a missing signature asset.
    let verification_enabled = env::var(UPDATE_MINISIGN_PUBKEY)
        .ok()
        .is_some_and(|s| !s.trim().is_empty());
    if !verification_enabled {
        return Ok(false);
    }

    if let Some(dir) = &options.file_release_dir {
        let src = dir.join(names.signature);
        if src.is_file() {
            fs::copy(&src, dest)?;
            return Ok(true);
        }
        return Ok(false);
    }

    // Network path: look the signature up on the same release. We tolerate a
    // missing asset (older/unsigned releases) but surface real download errors.
    let client = http_client()?;
    let api_url = match options.version.as_deref() {
        Some(tag) => format!(
            "https://api.github.com/repos/{}/releases/tags/{tag}",
            options.repo
        ),
        None => format!(
            "https://api.github.com/repos/{}/releases/latest",
            options.repo
        ),
    };
    let release: GithubRelease = client
        .get(&api_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    match release
        .assets
        .iter()
        .find(|asset| asset.name == names.signature)
    {
        Some(asset) => {
            download_to_file(client, &asset.browser_download_url, dest).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Verify the detached signature when both a public key (`AXON_UPDATE_MINISIGN_PUBKEY`)
/// and a signature file are available. Inert otherwise — returns `Ok(())`.
///
/// When enforcement is active, a missing/invalid signature is a hard failure:
/// once an operator opts in by setting the public key, the updater must not
/// silently fall back to SHA256-only. Shells out to `minisign` to avoid adding
/// a crypto crate in this bounded pass.
pub(super) fn verify_optional_signature(
    archive_path: &Path,
    signature_path: &Path,
    signature_available: bool,
) -> Result<(), Box<dyn Error>> {
    let Some(pubkey) = env::var(UPDATE_MINISIGN_PUBKEY)
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        // No public key configured — signature verification disabled (inert).
        return Ok(());
    };

    if !signature_available {
        return Err(err(format!(
            "{UPDATE_MINISIGN_PUBKEY} is set but the release has no signature asset; \
             refusing to install an unsigned artifact"
        )));
    }

    // `minisign -V -P <pubkey> -m <archive> -x <sig>` verifies the detached sig.
    let output = std::process::Command::new("minisign")
        .arg("-V")
        .arg("-P")
        .arg(&pubkey)
        .arg("-m")
        .arg(archive_path)
        .arg("-x")
        .arg(signature_path)
        .output()
        .map_err(|e| {
            err(format!(
                "{UPDATE_MINISIGN_PUBKEY} is set but `minisign` is not runnable: {e}"
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(err(format!(
            "release signature verification failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}
