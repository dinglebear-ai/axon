use crate::core::config::Config;
use crate::core::http::http_client;
use crate::core::paths::axon_home_dir;
use crate::core::ui::{accent, muted, primary};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

const DEFAULT_REPO: &str = "jmagar/axon";
const UPDATE_FILE_RELEASE_DIR: &str = "AXON_UPDATE_FILE_RELEASE_DIR";
const UPDATE_INSTALL_PATH: &str = "AXON_UPDATE_INSTALL_PATH";
const DEV_TARGET_DIR: &str = "AXON_DEV_TARGET_DIR";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

// Release-artifact integrity (SHA256 + optional OPS-H3 signature) lives in the
// integrity submodule. Re-exported so the sidecar tests' `super::*` resolves
// the verification helpers unchanged.
mod integrity;
#[cfg(test)]
use integrity::verify_sha256;
use integrity::{
    parse_sha256_sidecar, resolve_optional_signature, verify_optional_signature, verify_sha256_file,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseAssetNames {
    archive: &'static str,
    checksum: &'static str,
    /// Optional detached minisign signature sidecar (OPS-H3). Present in the
    /// release only once signing is enabled; the updater treats it as optional.
    signature: &'static str,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
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
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "version": report.version,
            "install_path": report.install_path,
            "installed": report.installed,
            "container_synced": report.container_synced,
        }))?;
        println!("{json}");
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
    let names = release_asset_names(env::consts::OS, env::consts::ARCH)?;
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
        download_release_assets(
            &options.repo,
            options.version.as_deref(),
            &names,
            &archive_path,
        )
        .await?
    };

    let expected = parse_sha256_sidecar(&checksum_body)?;
    verify_sha256_file(&archive_path, &expected)?;

    // OPS-H3 (bounded): optional, independent-trust-root signature check on top
    // of the SHA256 (which shares a trust root with the binary). Resolves the
    // detached signature best-effort; verification is enforced only when a
    // public key is configured AND a signature is present — otherwise inert.
    let signature_path = temp.path().join(names.signature);
    let signature_available = resolve_optional_signature(&options, &names, &signature_path).await?;
    verify_optional_signature(&archive_path, &signature_path, signature_available)?;

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
        version,
        install_path: options.install_path,
        installed: !already_current,
        container_synced,
    })
}

async fn download_release_assets(
    repo: &str,
    version: Option<&str>,
    names: &ReleaseAssetNames,
    archive_path: &Path,
) -> Result<(String, String), Box<dyn Error>> {
    let client = http_client()?;
    let api_url = match version {
        Some(tag) => format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };

    let release: GithubRelease = client
        .get(&api_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let archive_url = find_asset_url(&release, names.archive)?;
    let checksum_url = find_asset_url(&release, names.checksum)?;
    download_to_file(client, archive_url, archive_path).await?;
    let checksum = client
        .get(checksum_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok((release.tag_name, checksum))
}

async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), Box<dyn Error>> {
    let response = client.get(url).send().await?.error_for_status()?;
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }
    file.flush().await?;
    Ok(())
}

fn find_asset_url<'a>(release: &'a GithubRelease, name: &str) -> Result<&'a str, Box<dyn Error>> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or_else(|| {
            err(format!(
                "release {} is missing asset {name}",
                release.tag_name
            ))
        })
}

fn release_asset_names(os: &str, arch: &str) -> Result<ReleaseAssetNames, Box<dyn Error>> {
    match (os, arch) {
        ("linux", "x86_64") => Ok(ReleaseAssetNames {
            archive: "axon-linux-x86_64.tar.gz",
            checksum: "axon-linux-x86_64.tar.gz.sha256",
            signature: "axon-linux-x86_64.tar.gz.minisig",
        }),
        _ => Err(err(format!(
            "unsupported platform for axon update: {os}/{arch}; only linux/x86_64 is wired"
        ))),
    }
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
    if !installed_binary.is_file() {
        return false;
    }
    let Ok(Ok(output)) = timeout(
        COMMAND_TIMEOUT,
        Command::new(installed_binary).arg("--version").output(),
    )
    .await
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    output_reports_version(&stdout, version) || output_reports_version(&stderr, version)
}

fn output_reports_version(output: &str, version: &str) -> bool {
    let target = normalize_version(version);
    output
        .split_whitespace()
        .any(|token| normalize_version(token) == target)
}

fn normalize_version(version: &str) -> &str {
    version.trim().trim_start_matches('v')
}

fn build_container_sync_command_with_paths(
    installed_binary: &Path,
    paths: ComposePaths,
) -> Result<SyncCommand, Box<dyn Error>> {
    let bin_dir = installed_binary
        .parent()
        .ok_or_else(|| {
            err(format!(
                "installed binary has no parent: {}",
                installed_binary.display()
            ))
        })?
        .to_path_buf();

    Ok(SyncCommand {
        program: "docker",
        args: compose_args(&paths, true),
        current_dir: paths.compose_dir,
        env_name: DEV_TARGET_DIR,
        env_value: bin_dir,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComposePaths {
    compose_dir: PathBuf,
    compose_file: PathBuf,
    env_file: Option<PathBuf>,
}

fn compose_args(paths: &ComposePaths, include_up: bool) -> Vec<String> {
    let mut args = vec!["compose".to_string()];
    if let Some(env_file) = &paths.env_file {
        args.push("--env-file".to_string());
        args.push(env_file.display().to_string());
    }
    args.push("-f".to_string());
    args.push(paths.compose_file.display().to_string());
    if include_up {
        args.extend(
            [
                "up",
                "-d",
                "axon",
                "--no-deps",
                "--no-build",
                "--force-recreate",
            ]
            .into_iter()
            .map(String::from),
        );
    }
    args
}

fn resolve_compose_paths() -> Result<ComposePaths, Box<dyn Error>> {
    let axon_home = axon_home_dir().ok_or_else(|| {
        err("HOME is unset or invalid; cannot resolve trusted ~/.axon compose assets")
    })?;
    resolve_compose_paths_from_home(&axon_home, env::var_os("AXON_ENV_FILE").map(PathBuf::from))
}

fn resolve_compose_paths_from_home(
    axon_home: &Path,
    explicit_env_file: Option<PathBuf>,
) -> Result<ComposePaths, Box<dyn Error>> {
    let compose_dir = axon_home.join("compose");
    let compose_file = compose_dir.join("docker-compose.yaml");
    if !compose_file.is_file() {
        return Err(err(format!(
            "trusted compose file is missing: {}; run axon setup init",
            compose_file.display()
        )));
    }
    Ok(ComposePaths {
        compose_dir,
        compose_file,
        env_file: resolve_axon_env_file(axon_home, explicit_env_file.as_deref()),
    })
}

fn resolve_axon_env_file(axon_home: &Path, explicit_env_file: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit_env_file {
        if path.is_absolute() && path.is_file() {
            return Some(path.to_path_buf());
        }
        return None;
    }
    let home_env = axon_home.join(".env");
    if home_env.is_file() {
        return Some(home_env);
    }
    None
}

async fn sync_container_from_installed_binary(
    installed_binary: &Path,
    paths: ComposePaths,
) -> Result<(), Box<dyn Error>> {
    let sync = build_container_sync_command_with_paths(installed_binary, paths)?;
    let mut command = Command::new(sync.program);
    command
        .args(&sync.args)
        .current_dir(&sync.current_dir)
        .env(sync.env_name, sync.env_value);
    let status = run_command(command, "container sync").await?;
    if !status.success() {
        return Err(err(format!("container sync failed with status {status}")));
    }

    Ok(())
}

async fn run_command(
    mut command: Command,
    description: &str,
) -> Result<std::process::ExitStatus, Box<dyn Error>> {
    match timeout(COMMAND_TIMEOUT, command.status()).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(err(format!(
            "{description} timed out after {} seconds",
            COMMAND_TIMEOUT.as_secs()
        ))),
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod update_tests;
