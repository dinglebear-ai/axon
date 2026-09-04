//! Release-artifact integrity verification for the self-updater.
//!
//! Two independent checks:
//! - **SHA256** (`parse_sha256_sidecar` + `verify_sha256_file`): always enforced;
//!   shares a trust root with the binary (both from the same release).
//! - **Signature** (`resolve_signature` + `verify_signature`):
//!   a mandatory detached minisign signature checked against the public key
//!   embedded in release builds, giving the updater an independent trust root.

use super::{GithubRelease, ReleaseAssetNames, UpdateOptions, download_to_file, err};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::Path;

/// Build-time environment variable containing the reviewed minisign public key.
pub(super) const UPDATE_MINISIGN_PUBKEY: &str = "AXON_UPDATE_MINISIGN_PUBKEY";
const EMBEDDED_UPDATE_MINISIGN_PUBKEY: Option<&str> = option_env!("AXON_UPDATE_MINISIGN_PUBKEY");

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

/// Resolve the detached signature into `dest`, returning whether one was found.
/// Missing signatures are rejected by `verify_signature`.
pub(super) async fn resolve_signature(
    options: &UpdateOptions,
    names: &ReleaseAssetNames,
    selected_release: Option<&GithubRelease>,
    dest: &Path,
) -> Result<bool, Box<dyn Error>> {
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
    let release = selected_release
        .ok_or_else(|| err("network update signature lookup has no selected release"))?;
    let client = axon_core::http::http_client()?;
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
pub(super) fn verify_signature(
    archive_path: &Path,
    signature_path: &Path,
    signature_available: bool,
) -> Result<(), Box<dyn Error>> {
    let pubkey = EMBEDDED_UPDATE_MINISIGN_PUBKEY
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| err(format!(
            "this build has no embedded {UPDATE_MINISIGN_PUBKEY}; refusing to install an unverifiable artifact"
        )))?;

    if !signature_available {
        return Err(err(
            "the release has no signature asset; refusing to install an unsigned artifact"
                .to_string(),
        ));
    }

    // `minisign -V -P <pubkey> -m <archive> -x <sig>` verifies the detached sig.
    let output = std::process::Command::new("minisign")
        .arg("-V")
        .arg("-P")
        .arg(pubkey)
        .arg("-m")
        .arg(archive_path)
        .arg("-x")
        .arg(signature_path)
        .output()
        .map_err(|e| {
            err(format!(
                "signature verification requires `minisign`, but it is not runnable: {e}"
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
