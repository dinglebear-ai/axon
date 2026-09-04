use axon_core::config::Config;
use axon_core::http::http_client;
use axon_core::ui::{accent, muted, primary};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

const DEFAULT_REPO: &str = "dinglebear-ai/axon";
const UPDATE_FILE_RELEASE_DIR: &str = "AXON_UPDATE_FILE_RELEASE_DIR";
const UPDATE_INSTALL_PATH: &str = "AXON_UPDATE_INSTALL_PATH";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

// Release-artifact integrity (SHA256 + mandatory minisign signature) lives in the
// integrity submodule. Re-exported so the sidecar tests' `super::*` resolves
// the verification helpers unchanged.
mod container_sync;
mod integrity;
mod release;

#[cfg(test)]
use container_sync::{
    ComposePaths, build_container_sync_command_with_paths, compose_args, resolve_axon_env_file,
    resolve_compose_paths_from_home,
};
use container_sync::{resolve_compose_paths, sync_container_from_installed_binary};
#[cfg(test)]
use integrity::verify_sha256;
use integrity::{parse_sha256_sidecar, resolve_signature, verify_sha256_file, verify_signature};
#[cfg(test)]
use release::select_latest_compatible_release;
use release::{
    download_release_assets, download_to_file, fetch_compatible_release, release_asset_names,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseAssetNames {
    archive: &'static str,
    checksum: &'static str,
    /// Required detached minisign signature sidecar.
    signature: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
struct UpdateOptions {
    repo: String,
    version: Option<String>,
    force: bool,
    sync_container: bool,
    install_path: PathBuf,
    file_release_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct UpdateReport {
    version: String,
    install_path: PathBuf,
    installed: bool,
    container_synced: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct SyncCommand {
    program: &'static str,
    args: Vec<String>,
    current_dir: PathBuf,
    env_name: &'static str,
    env_value: PathBuf,
}

#[derive(Debug)]
struct UpdateError(String);

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for UpdateError {}

fn err(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(UpdateError(message.into()))
}

pub async fn run_update(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let options = parse_update_options(cfg)?;
    let report = perform_update(options).await?;

    if cfg.json_output {
        crate::json::print_json_gated(&serde_json::json!({
            "version": report.version,
            "install_path": report.install_path,
            "installed": report.installed,
            "container_synced": report.container_synced,
        }))?;
    } else {
        if report.installed {
            println!("{}", primary(&format!("installed axon {}", report.version)));
        } else {
            println!(
                "{}",
                muted(&format!("axon {} already installed", report.version))
            );
        }
        println!("{} {}", accent("path:"), report.install_path.display());
        if report.container_synced {
            println!("{}", primary("container synced"));
        } else {
            println!("{}", muted("container sync skipped"));
        }
    }

    Ok(())
}

fn parse_update_options(cfg: &Config) -> Result<UpdateOptions, Box<dyn Error>> {
    let mut repo = DEFAULT_REPO.to_string();
    let mut version = None;
    let mut force = false;
    let mut sync_container = true;

    let mut args = cfg.positional.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                repo = args
                    .next()
                    .ok_or_else(|| err("--repo requires a value"))?
                    .to_string();
            }
            "--version" => {
                version = Some(
                    args.next()
                        .ok_or_else(|| err("--version requires a value"))?
                        .to_string(),
                );
            }
            "--force" => force = true,
            "--no-container" => sync_container = false,
            other => return Err(err(format!("unknown update argument: {other}"))),
        }
    }

    let install_path = env::var_os(UPDATE_INSTALL_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(default_install_path);

    Ok(UpdateOptions {
        repo,
        version,
        force,
        sync_container,
        install_path,
        file_release_dir: env::var_os(UPDATE_FILE_RELEASE_DIR).map(PathBuf::from),
    })
}

fn default_install_path() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/bin/axon")
}

async fn perform_update(options: UpdateOptions) -> Result<UpdateReport, Box<dyn Error>> {
    perform_update_inner(
        options,
        env::consts::OS,
        env::consts::ARCH,
        SignaturePolicy::Required,
    )
    .await
}

#[derive(Clone, Copy)]
enum SignaturePolicy {
    Required,
    #[cfg(test)]
    SkipForFixture,
}

#[cfg(test)]
async fn perform_fixture_update(options: UpdateOptions) -> Result<UpdateReport, Box<dyn Error>> {
    perform_update_inner(options, "linux", "x86_64", SignaturePolicy::SkipForFixture).await
}

async fn perform_update_inner(
    options: UpdateOptions,
    os: &str,
    arch: &str,
    signature_policy: SignaturePolicy,
) -> Result<UpdateReport, Box<dyn Error>> {
    let names = release_asset_names(os, arch)?;
    let client = if options.file_release_dir.is_none() {
        Some(http_client()?)
    } else {
        None
    };
    let selected_release = match client {
        Some(client) => Some(
            fetch_compatible_release(client, &options.repo, options.version.as_deref(), &names)
                .await?,
        ),
        None => None,
    };

    if options.file_release_dir.is_none() && options.version.is_none() {
        let release = selected_release
            .as_ref()
            .ok_or_else(|| err("automatic update did not resolve a release"))?;
        let installed_version = installed_binary_version(&options.install_path).await;
        if should_skip_automatic_update(
            &release.tag_name,
            installed_version.as_deref(),
            options.force,
        )? {
            let mut container_synced = false;
            if options.sync_container {
                let compose_paths = resolve_compose_paths()?;
                sync_container_from_installed_binary(&options.install_path, compose_paths).await?;
                container_synced = true;
            }
            return Ok(UpdateReport {
                version: normalize_version(
                    installed_version
                        .as_deref()
                        .unwrap_or(release.tag_name.as_str()),
                )
                .to_string(),
                install_path: options.install_path,
                installed: false,
                container_synced,
            });
        }
    }
    let temp = tempfile::tempdir()?;
    let archive_path = temp.path().join(names.archive);
    let compose_paths = if options.sync_container {
        Some(resolve_compose_paths()?)
    } else {
        None
    };

    let (version, checksum_body) = if let Some(dir) = &options.file_release_dir {
        fs::copy(dir.join(names.archive), &archive_path)?;
        let checksum = fs::read_to_string(dir.join(names.checksum))?;
        (
            options
                .version
                .clone()
                .unwrap_or_else(|| "local-test-release".to_string()),
            checksum,
        )
    } else {
        let client =
            client.ok_or_else(|| err("network update did not initialize an HTTP client"))?;
        let release = selected_release
            .as_ref()
            .ok_or_else(|| err("network update did not resolve a release"))?;
        download_release_assets(client, release, &names, &archive_path).await?
    };

    let expected = parse_sha256_sidecar(&checksum_body)?;
    verify_sha256_file(&archive_path, &expected)?;

    // Mandatory independent-trust-root signature check on top of SHA256.
    if matches!(signature_policy, SignaturePolicy::Required) {
        let signature_path = temp.path().join(names.signature);
        let signature_available =
            resolve_signature(&options, &names, selected_release.as_ref(), &signature_path).await?;
        verify_signature(&archive_path, &signature_path, signature_available)?;
    }

    let already_current =
        !options.force && installed_binary_reports_version(&options.install_path, &version).await;
    if !already_current {
        let extracted = extract_axon_binary(&archive_path, temp.path())?;
        install_binary_atomically(&extracted, &options.install_path)?;
    }

    let mut container_synced = false;
    if let Some(compose_paths) = compose_paths {
        sync_container_from_installed_binary(&options.install_path, compose_paths).await?;
        container_synced = true;
    }

    Ok(UpdateReport {
        version: normalize_version(&version).to_string(),
        install_path: options.install_path,
        installed: !already_current,
        container_synced,
    })
}

fn should_skip_automatic_update(
    release_tag: &str,
    installed_version: Option<&str>,
    force: bool,
) -> Result<bool, Box<dyn Error>> {
    Ok(!force
        && installed_version
            .map(|installed| release_is_newer_than(release_tag, installed).map(|newer| !newer))
            .transpose()?
            .unwrap_or(false))
}

fn release_is_newer_than(tag: &str, current: &str) -> Result<bool, Box<dyn Error>> {
    Ok(version_tuple(tag)? > version_tuple(current)?)
}

fn version_tuple(raw: &str) -> Result<(u64, u64, u64), Box<dyn Error>> {
    let core = raw
        .trim()
        .trim_start_matches('v')
        .split_once('-')
        .map_or_else(|| raw.trim().trim_start_matches('v'), |(core, _)| core);
    let mut parts = core.split('.');
    let major = parts
        .next()
        .ok_or_else(|| err(format!("invalid release version: {raw}")))?
        .parse()?;
    let minor = parts
        .next()
        .ok_or_else(|| err(format!("invalid release version: {raw}")))?
        .parse()?;
    let patch = parts
        .next()
        .ok_or_else(|| err(format!("invalid release version: {raw}")))?
        .parse()?;
    if parts.next().is_some() {
        return Err(err(format!("invalid release version: {raw}")));
    }
    Ok((major, minor, patch))
}

fn extract_axon_binary(archive_path: &Path, temp_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let file = fs::File::open(archive_path)?;
    let gz = GzDecoder::new(BufReader::new(file));
    let mut archive = Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.as_ref() == Path::new("axon") {
            let output = temp_dir.join("axon");
            entry.unpack(&output)?;
            return Ok(output);
        }
    }

    Err(err("release archive did not contain executable axon"))
}

fn install_binary_atomically(source: &Path, dest: &Path) -> Result<(), Box<dyn Error>> {
    let parent = dest
        .parent()
        .ok_or_else(|| err(format!("install path has no parent: {}", dest.display())))?;
    fs::create_dir_all(parent)?;

    let temp_dest = parent.join(format!(
        ".{}.tmp-{}-{}",
        dest.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("axon"),
        std::process::id(),
        unique_suffix()
    ));

    fs::copy(source, &temp_dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&temp_dest)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&temp_dest, permissions)?;
    }
    fs::rename(&temp_dest, dest)?;
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

async fn installed_binary_reports_version(installed_binary: &Path, version: &str) -> bool {
    installed_binary_version(installed_binary)
        .await
        .is_some_and(|installed| normalize_version(&installed) == normalize_version(version))
}

async fn installed_binary_version(installed_binary: &Path) -> Option<String> {
    if !installed_binary.is_file() {
        return None;
    }
    let Ok(Ok(output)) = timeout(
        COMMAND_TIMEOUT,
        Command::new(installed_binary).arg("--version").output(),
    )
    .await
    else {
        return None;
    };
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    output_version(&stdout).or_else(|| output_version(&stderr))
}

#[cfg(test)]
fn output_reports_version(output: &str, version: &str) -> bool {
    let target = normalize_version(version);
    output
        .split_whitespace()
        .any(|token| normalize_version(token) == target)
}

fn output_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| version_tuple(token).is_ok())
        .map(|token| normalize_version(token).to_string())
}

fn normalize_version(version: &str) -> &str {
    version.trim().trim_start_matches('v')
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod update_tests;
