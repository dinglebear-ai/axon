# Axon Update Release Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `axon update` so a local user can download the latest Linux binary from GitHub Releases, install it into PATH, and sync the running Axon container to that same binary.

**Architecture:** Add a small CLI command module that resolves the latest GitHub Release asset, verifies the published SHA256 sidecar, atomically installs the `axon` executable into `~/.local/bin`, and then reuses the repo's existing Docker Compose sync path. Keep release-download logic testable as pure URL/asset/checksum helpers, and keep filesystem/container mutation behind a narrow command runner so tests can use a fake release directory and fake commands.

**Tech Stack:** Rust 1.94, clap, reqwest, sha2, tar/flate2, tempfile, existing `docker compose` and Justfile runtime conventions.

---

## File Structure

- Modify `Cargo.toml` to add `sha2 = "0.10"` if it is not already present.
- Modify `src/core/config/cli.rs` to add the `update` subcommand and its flags.
- Modify `src/core/config/types/enums.rs` to add `CommandKind::Update`.
- Modify `src/core/config/parse/build_config/command_dispatch.rs` to route `CliCommand::Update`.
- Modify `src/lib.rs` to dispatch `CommandKind::Update`.
- Modify `src/cli/commands.rs` to export the new module.
- Create `src/cli/commands/update.rs` for the CLI entry point, install orchestration, and command execution boundary.
- Create `src/cli/commands/update_tests.rs` for focused unit tests using local fake release artifacts and fake command scripts.
- Modify `Justfile` only if the implementation needs a helper that syncs a preinstalled release binary into the container without rebuilding from source.
- Modify `docs/reference/actions/setup.md` or `docs/operations/deployment.md` to document `axon update` after the command is working.

## Behavior Contract

- Default command: `axon update`
- Default repo: `jmagar/axon`
- Default tag: latest release from `https://api.github.com/repos/jmagar/axon/releases/latest`
- Default asset on Linux x86_64: `axon-linux-x86_64.tar.gz`
- Required checksum asset: `axon-linux-x86_64.tar.gz.sha256`
- Default install destination: `~/.local/bin/axon`
- Default container sync: enabled
- Safe overwrite: download to temp dir, verify checksum, extract `axon`, install to a temp sibling, rename atomically over destination, chmod executable.
- Failure behavior: if download/checksum/install fails, leave existing PATH binary untouched and do not sync the container.
- Unsupported platform: return a clear error for non-Linux or non-x86_64 until those assets are intentionally wired.
- Idempotency: if the installed binary already reports the target version and `--force` is absent, skip install but still allow container sync when requested.

## Task 1: Add CLI Shape and Dispatch

**Files:**
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `src/lib.rs`
- Modify: `src/cli/commands.rs`
- Create: `src/cli/commands/update.rs`
- Create: `src/cli/commands/update_tests.rs`

- [ ] **Step 1: Write the failing parse and command-kind tests**

Add these tests to `src/core/config/parse_tests.rs`:

```rust
#[test]
fn update_defaults_to_latest_release_and_container_sync() {
    let cfg = super::parse_args_from(["axon", "update"]).unwrap();

    assert!(matches!(cfg.command, CommandKind::Update));
    assert_eq!(cfg.positional, Vec::<String>::new());
}

#[test]
fn update_accepts_version_repo_no_container_and_force_flags() {
    let cfg = super::parse_args_from([
        "axon",
        "update",
        "--version",
        "v5.9.2",
        "--repo",
        "jmagar/axon",
        "--no-container",
        "--force",
    ])
    .unwrap();

    assert!(matches!(cfg.command, CommandKind::Update));
    assert_eq!(
        cfg.positional,
        vec![
            "--version".to_string(),
            "v5.9.2".to_string(),
            "--repo".to_string(),
            "jmagar/axon".to_string(),
            "--no-container".to_string(),
            "--force".to_string(),
        ]
    );
}
```

- [ ] **Step 2: Run the focused failing tests**

Run:

```bash
cargo test --locked update_defaults_to_latest_release_and_container_sync update_accepts_version_repo_no_container_and_force_flags
```

Expected: FAIL because `CommandKind::Update`, `CliCommand::Update`, and routing do not exist yet.

- [ ] **Step 3: Add CLI structs and command kind**

In `src/core/config/cli.rs`, add this enum variant near the other top-level maintenance commands:

```rust
    /// Download and install the latest GitHub Release binary, then sync the local container
    Update(UpdateArgs),
```

Add this args struct near the other small command arg structs:

```rust
#[derive(Debug, Args)]
pub(super) struct UpdateArgs {
    /// GitHub repository in owner/name form.
    #[arg(long, default_value = "jmagar/axon")]
    pub(super) repo: String,

    /// Release tag to install. Defaults to the latest GitHub Release.
    #[arg(long)]
    pub(super) version: Option<String>,

    /// Install even when the destination already reports the target version.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(super) force: bool,

    /// Do not restart/sync the local Axon container after installing.
    #[arg(long = "no-container", action = ArgAction::SetTrue)]
    pub(super) no_container: bool,
}
```

In `src/core/config/types/enums.rs`, add:

```rust
    Update,
```

and in `CommandKind::as_str()`:

```rust
            Self::Update => "update",
```

- [ ] **Step 4: Route CLI args into `Config::positional`**

In `src/core/config/parse/build_config/command_dispatch.rs`, add a match arm:

```rust
        CliCommand::Update(args) => {
            out.command = CommandKind::Update;
            if let Some(version) = args.version {
                out.positional.push("--version".to_string());
                out.positional.push(version);
            }
            if args.repo != "jmagar/axon" {
                out.positional.push("--repo".to_string());
                out.positional.push(args.repo);
            }
            if args.no_container {
                out.positional.push("--no-container".to_string());
            }
            if args.force {
                out.positional.push("--force".to_string());
            }
        }
```

If the second parse test expects default repo to be preserved in positionals, either remove the `args.repo != "jmagar/axon"` guard or update the test to assert only non-default args. Prefer the guard so defaults do not add noise.

- [ ] **Step 5: Add a stub command module**

In `src/cli/commands.rs`, add:

```rust
pub mod update;
pub use update::run_update;
```

Create `src/cli/commands/update.rs`:

```rust
use crate::core::config::Config;
use std::error::Error;

pub async fn run_update(_cfg: &Config) -> Result<(), Box<dyn Error>> {
    Err("axon update is not implemented yet".into())
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod update_tests;
```

In `src/lib.rs`, import `run_update` and add the dispatch arm:

```rust
        CommandKind::Update => run_update(cfg).await?,
```

- [ ] **Step 6: Run focused parse tests**

Run:

```bash
cargo test --locked update_defaults_to_latest_release_and_container_sync update_accepts_version_repo_no_container_and_force_flags
```

Expected: PASS.

- [ ] **Step 7: Commit the CLI shell**

Run:

```bash
git add src/core/config/cli.rs src/core/config/types/enums.rs src/core/config/parse/build_config/command_dispatch.rs src/lib.rs src/cli/commands.rs src/cli/commands/update.rs src/core/config/parse_tests.rs
git commit -m "feat(cli): add axon update command shell"
```

## Task 2: Implement Release Asset Resolution and Checksum Verification

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/cli/commands/update.rs`
- Modify: `src/cli/commands/update_tests.rs`

- [ ] **Step 1: Add pure helper tests**

Create `src/cli/commands/update_tests.rs` with:

```rust
use super::*;

#[test]
fn linux_x86_64_release_asset_names_are_expected() {
    let names = release_asset_names("linux", "x86_64").unwrap();

    assert_eq!(names.archive, "axon-linux-x86_64.tar.gz");
    assert_eq!(names.checksum, "axon-linux-x86_64.tar.gz.sha256");
}

#[test]
fn unsupported_platform_returns_clear_error() {
    let err = release_asset_names("darwin", "aarch64").unwrap_err();

    assert!(err.to_string().contains("unsupported platform"));
    assert!(err.to_string().contains("darwin/aarch64"));
}

#[test]
fn parses_sha256_sidecar_with_filename() {
    let parsed = parse_sha256_sidecar(
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08  axon-linux-x86_64.tar.gz\n",
    )
    .unwrap();

    assert_eq!(
        parsed,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
}

#[test]
fn checksum_mismatch_is_rejected() {
    let err = verify_sha256(b"test", "0000000000000000000000000000000000000000000000000000000000000000")
        .unwrap_err();

    assert!(err.to_string().contains("checksum mismatch"));
}

#[test]
fn checksum_match_is_accepted() {
    verify_sha256(
        b"test",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    )
    .unwrap();
}
```

- [ ] **Step 2: Run the failing helper tests**

Run:

```bash
cargo test --locked linux_x86_64_release_asset_names_are_expected unsupported_platform_returns_clear_error parses_sha256_sidecar_with_filename checksum_mismatch_is_rejected checksum_match_is_accepted
```

Expected: FAIL because the helper functions do not exist.

- [ ] **Step 3: Add checksum dependency**

In `Cargo.toml`, add:

```toml
sha2 = "0.10"
```

- [ ] **Step 4: Implement helper types and functions**

Replace the stub body in `src/cli/commands/update.rs` with:

```rust
use crate::core::config::Config;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tar::Archive;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseAssetNames {
    archive: &'static str,
    checksum: &'static str,
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

fn release_asset_names(os: &str, arch: &str) -> Result<ReleaseAssetNames, Box<dyn Error>> {
    match (os, arch) {
        ("linux", "x86_64") => Ok(ReleaseAssetNames {
            archive: "axon-linux-x86_64.tar.gz",
            checksum: "axon-linux-x86_64.tar.gz.sha256",
        }),
        _ => Err(err(format!(
            "unsupported platform for axon update: {os}/{arch}; only linux/x86_64 is wired"
        ))),
    }
}

fn parse_sha256_sidecar(body: &str) -> Result<String, Box<dyn Error>> {
    let hash = body
        .split_whitespace()
        .next()
        .ok_or_else(|| err("empty sha256 sidecar"))?;
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err(format!("invalid sha256 sidecar hash: {hash}")));
    }
    Ok(hash.to_ascii_lowercase())
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), Box<dyn Error>> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected.to_ascii_lowercase() {
        return Err(err(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}
```

Keep the existing `run_update()` stub below these helpers for now.

- [ ] **Step 5: Run helper tests**

Run:

```bash
cargo test --locked linux_x86_64_release_asset_names_are_expected unsupported_platform_returns_clear_error parses_sha256_sidecar_with_filename checksum_mismatch_is_rejected checksum_match_is_accepted
```

Expected: PASS.

- [ ] **Step 6: Commit helper layer**

Run:

```bash
git add Cargo.toml Cargo.lock src/cli/commands/update.rs src/cli/commands/update_tests.rs
git commit -m "feat(update): resolve release assets and verify checksums"
```

## Task 3: Download, Extract, and Atomically Install the Binary

**Files:**
- Modify: `src/cli/commands/update.rs`
- Modify: `src/cli/commands/update_tests.rs`

- [ ] **Step 1: Add extraction and atomic install tests**

Append to `src/cli/commands/update_tests.rs`:

```rust
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::Write;
use tar::{Builder, Header};

fn make_release_archive(script_body: &str) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = Builder::new(&mut tar_bytes);
        let mut header = Header::new_gnu();
        header.set_path("axon").unwrap();
        header.set_size(script_body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append(&header, script_body.as_bytes())
            .expect("append axon");
        builder.finish().expect("finish tar");
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn extracts_axon_binary_from_release_archive() {
    let archive = make_release_archive("#!/usr/bin/env sh\necho axon 5.9.2\n");
    let temp = tempfile::tempdir().unwrap();
    let extracted = extract_axon_binary(&archive, temp.path()).unwrap();

    assert_eq!(fs::read_to_string(&extracted).unwrap(), "#!/usr/bin/env sh\necho axon 5.9.2\n");
}

#[test]
fn atomic_install_replaces_destination_and_sets_executable_mode() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("axon-new");
    let dest = temp.path().join("bin").join("axon");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&source, "#!/usr/bin/env sh\necho new\n").unwrap();
    fs::write(&dest, "#!/usr/bin/env sh\necho old\n").unwrap();

    install_binary_atomically(&source, &dest).unwrap();

    assert_eq!(fs::read_to_string(&dest).unwrap(), "#!/usr/bin/env sh\necho new\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&dest).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}
```

- [ ] **Step 2: Run failing extraction/install tests**

Run:

```bash
cargo test --locked extracts_axon_binary_from_release_archive atomic_install_replaces_destination_and_sets_executable_mode
```

Expected: FAIL because extraction and install helpers do not exist.

- [ ] **Step 3: Implement extraction and atomic install**

Add to `src/cli/commands/update.rs`:

```rust
fn extract_axon_binary(archive_bytes: &[u8], temp_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let gz = GzDecoder::new(Cursor::new(archive_bytes));
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
        ".{}.tmp-{}",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("axon"),
        std::process::id()
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
```

- [ ] **Step 4: Run extraction/install tests**

Run:

```bash
cargo test --locked extracts_axon_binary_from_release_archive atomic_install_replaces_destination_and_sets_executable_mode
```

Expected: PASS.

- [ ] **Step 5: Commit install primitives**

Run:

```bash
git add src/cli/commands/update.rs src/cli/commands/update_tests.rs
git commit -m "feat(update): install downloaded axon binary atomically"
```

## Task 4: Wire the End-to-End `axon update` Command

**Files:**
- Modify: `src/cli/commands/update.rs`
- Modify: `src/cli/commands/update_tests.rs`

- [ ] **Step 1: Add local fake-release end-to-end tests**

Append to `src/cli/commands/update_tests.rs`:

```rust
#[tokio::test]
async fn update_installs_from_file_base_url_without_container_sync() {
    let temp = tempfile::tempdir().unwrap();
    let release = make_release_archive("#!/usr/bin/env sh\necho axon 5.9.2\n");
    let checksum = format!("{:x}", sha2::Sha256::digest(&release));
    let archive_path = temp.path().join("axon-linux-x86_64.tar.gz");
    let checksum_path = temp.path().join("axon-linux-x86_64.tar.gz.sha256");
    fs::write(&archive_path, &release).unwrap();
    fs::write(
        &checksum_path,
        format!("{checksum}  axon-linux-x86_64.tar.gz\n"),
    )
    .unwrap();

    let install_dir = temp.path().join("install");
    let options = UpdateOptions {
        repo: "jmagar/axon".to_string(),
        version: Some("v5.9.2".to_string()),
        force: true,
        sync_container: false,
        install_path: install_dir.join("axon"),
        file_release_dir: Some(temp.path().to_path_buf()),
    };

    let report = perform_update(options).await.unwrap();

    assert_eq!(report.version, "v5.9.2");
    assert_eq!(
        fs::read_to_string(install_dir.join("axon")).unwrap(),
        "#!/usr/bin/env sh\necho axon 5.9.2\n"
    );
    assert!(!report.container_synced);
}
```

- [ ] **Step 2: Run the failing end-to-end test**

Run:

```bash
cargo test --locked update_installs_from_file_base_url_without_container_sync
```

Expected: FAIL because `UpdateOptions`, `UpdateReport`, and `perform_update` do not exist.

- [ ] **Step 3: Implement options parsing and local-file test mode**

Add to `src/cli/commands/update.rs`:

```rust
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
    container_synced: bool,
}

fn parse_update_options(cfg: &Config) -> Result<UpdateOptions, Box<dyn Error>> {
    let mut repo = "jmagar/axon".to_string();
    let mut version = None;
    let mut force = false;
    let mut sync_container = true;
    let mut file_release_dir = std::env::var_os("AXON_UPDATE_FILE_RELEASE_DIR").map(PathBuf::from);

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

    let install_path = std::env::var_os("AXON_UPDATE_INSTALL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            home.join(".local/bin/axon")
        });

    Ok(UpdateOptions {
        repo,
        version,
        force,
        sync_container,
        install_path,
        file_release_dir: file_release_dir.take(),
    })
}
```

- [ ] **Step 4: Implement download and update orchestration**

Add to `src/cli/commands/update.rs`:

```rust
async fn perform_update(options: UpdateOptions) -> Result<UpdateReport, Box<dyn Error>> {
    let names = release_asset_names(std::env::consts::OS, std::env::consts::ARCH)?;
    let temp = tempfile::tempdir()?;

    let (version, archive_bytes, checksum_body) = if let Some(dir) = &options.file_release_dir {
        let archive = fs::read(dir.join(names.archive))?;
        let checksum = fs::read_to_string(dir.join(names.checksum))?;
        (
            options
                .version
                .clone()
                .unwrap_or_else(|| "local-test-release".to_string()),
            archive,
            checksum,
        )
    } else {
        download_release_assets(&options.repo, options.version.as_deref(), &names).await?
    };

    let expected = parse_sha256_sidecar(&checksum_body)?;
    verify_sha256(&archive_bytes, &expected)?;
    let extracted = extract_axon_binary(&archive_bytes, temp.path())?;
    install_binary_atomically(&extracted, &options.install_path)?;

    let mut container_synced = false;
    if options.sync_container {
        sync_container_from_installed_binary(&options.install_path)?;
        container_synced = true;
    }

    Ok(UpdateReport {
        version,
        install_path: options.install_path,
        container_synced,
    })
}
```

- [ ] **Step 5: Implement GitHub download**

Add to `src/cli/commands/update.rs`:

```rust
async fn download_release_assets(
    repo: &str,
    version: Option<&str>,
    names: &ReleaseAssetNames,
) -> Result<(String, Vec<u8>, String), Box<dyn Error>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("axon-update/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
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
    let archive = client
        .get(archive_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    let checksum = client
        .get(checksum_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok((release.tag_name, archive, checksum))
}

fn find_asset_url<'a>(
    release: &'a GithubRelease,
    name: &str,
) -> Result<&'a str, Box<dyn Error>> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or_else(|| err(format!("release {} is missing asset {name}", release.tag_name)))
}
```

- [ ] **Step 6: Implement command entry point output**

Replace `run_update()` with:

```rust
pub async fn run_update(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let options = parse_update_options(cfg)?;
    let report = perform_update(options).await?;

    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "version": report.version,
                "install_path": report.install_path,
                "container_synced": report.container_synced,
            })
        );
    } else {
        println!("installed axon {}", report.version);
        println!("path: {}", report.install_path.display());
        if report.container_synced {
            println!("container synced");
        } else {
            println!("container sync skipped");
        }
    }

    Ok(())
}
```

- [ ] **Step 7: Run end-to-end local-file test**

Run:

```bash
cargo test --locked update_installs_from_file_base_url_without_container_sync
```

Expected: PASS.

- [ ] **Step 8: Commit end-to-end update flow**

Run:

```bash
git add src/cli/commands/update.rs src/cli/commands/update_tests.rs
git commit -m "feat(update): download and install release binary"
```

## Task 5: Sync the Installed Release Binary Into the Container

**Files:**
- Modify: `src/cli/commands/update.rs`
- Modify: `src/cli/commands/update_tests.rs`
- Modify: `Justfile` if needed

- [ ] **Step 1: Decide the sync mechanism from current Compose behavior**

Inspect:

```bash
sed -n '126,195p' Justfile
sed -n '1,140p' docker-compose.yaml
```

Expected: `sync-container` exports `AXON_DEV_TARGET_DIR="$(dirname "$AXON_BIN")"` and starts the `axon` service with that target dir mounted at `/home/axon/.axon/dev`.

- [ ] **Step 2: Add command-runner boundary tests**

Append to `src/cli/commands/update_tests.rs`:

```rust
#[test]
fn sync_container_uses_installed_binary_directory_as_dev_target() {
    let temp = tempfile::tempdir().unwrap();
    let fake_bin = temp.path().join("bin").join("axon");
    fs::create_dir_all(fake_bin.parent().unwrap()).unwrap();
    fs::write(&fake_bin, "#!/usr/bin/env sh\necho axon 5.9.2\n").unwrap();

    let sync = build_container_sync_command(&fake_bin).unwrap();

    assert_eq!(sync.env_name, "AXON_DEV_TARGET_DIR");
    assert_eq!(sync.env_value, fake_bin.parent().unwrap());
    assert_eq!(sync.program, "docker");
    assert_eq!(
        sync.args,
        vec![
            "compose",
            "-f",
            "docker-compose.yaml",
            "up",
            "-d",
            "axon",
            "--no-deps",
            "--no-build",
        ]
    );
}
```

- [ ] **Step 3: Run failing sync command test**

Run:

```bash
cargo test --locked sync_container_uses_installed_binary_directory_as_dev_target
```

Expected: FAIL because `build_container_sync_command` does not exist.

- [ ] **Step 4: Implement sync command construction**

Add to `src/cli/commands/update.rs`:

```rust
#[derive(Debug, PartialEq, Eq)]
struct SyncCommand {
    program: &'static str,
    args: Vec<&'static str>,
    env_name: &'static str,
    env_value: PathBuf,
}

fn build_container_sync_command(installed_binary: &Path) -> Result<SyncCommand, Box<dyn Error>> {
    let bin_dir = installed_binary
        .parent()
        .ok_or_else(|| err(format!("installed binary has no parent: {}", installed_binary.display())))?
        .to_path_buf();

    Ok(SyncCommand {
        program: "docker",
        args: vec![
            "compose",
            "-f",
            "docker-compose.yaml",
            "up",
            "-d",
            "axon",
            "--no-deps",
            "--no-build",
        ],
        env_name: "AXON_DEV_TARGET_DIR",
        env_value: bin_dir,
    })
}

fn sync_container_from_installed_binary(installed_binary: &Path) -> Result<(), Box<dyn Error>> {
    let sync = build_container_sync_command(installed_binary)?;
    let status = std::process::Command::new(sync.program)
        .args(sync.args)
        .env(sync.env_name, sync.env_value)
        .status()?;
    if !status.success() {
        return Err(err(format!("container sync failed with status {status}")));
    }

    let restart_status = std::process::Command::new("docker")
        .args(["compose", "-f", "docker-compose.yaml", "restart", "axon"])
        .status()?;
    if !restart_status.success() {
        return Err(err(format!("container restart failed with status {restart_status}")));
    }

    Ok(())
}
```

- [ ] **Step 5: Preserve env-file behavior if required**

If `docker compose --env-file ~/.axon/.env` is required for local container sync, extend `SyncCommand` to include optional args from `scripts/lib/axon-env.sh` equivalent logic. Add this test:

```rust
#[test]
fn env_file_args_are_inserted_before_compose_file() {
    let args = compose_args(Some(Path::new("/home/j/.axon/.env")));

    assert_eq!(
        args,
        vec![
            "compose",
            "--env-file",
            "/home/j/.axon/.env",
            "-f",
            "docker-compose.yaml",
            "up",
            "-d",
            "axon",
            "--no-deps",
            "--no-build",
        ]
    );
}
```

Then implement `compose_args(env_file: Option<&Path>) -> Vec<String>` and use `std::process::Command` with owned `String` args.

- [ ] **Step 6: Run sync tests**

Run:

```bash
cargo test --locked sync_container_uses_installed_binary_directory_as_dev_target
```

Expected: PASS.

- [ ] **Step 7: Run a live no-container install with a fake release**

Run:

```bash
tmp="$(mktemp -d)"
printf '#!/usr/bin/env sh\necho axon fake-update\n' > "$tmp/axon"
chmod +x "$tmp/axon"
tar -C "$tmp" -czf "$tmp/axon-linux-x86_64.tar.gz" axon
sha256sum "$tmp/axon-linux-x86_64.tar.gz" > "$tmp/axon-linux-x86_64.tar.gz.sha256"
AXON_UPDATE_FILE_RELEASE_DIR="$tmp" \
AXON_UPDATE_INSTALL_PATH="$tmp/install/axon" \
cargo run --locked --bin axon -- update --version v0.0.0-test --no-container --force
"$tmp/install/axon"
```

Expected: command prints `installed axon v0.0.0-test`, skips container sync, and running the installed fake binary prints `axon fake-update`.

- [ ] **Step 8: Commit container sync integration**

Run:

```bash
git add src/cli/commands/update.rs src/cli/commands/update_tests.rs Justfile
git commit -m "feat(update): sync installed release into container"
```

## Task 6: Documentation and Final Verification

**Files:**
- Modify: `docs/operations/deployment.md`
- Modify: `README.md` if there is a CLI command table that lists maintenance commands.

- [ ] **Step 1: Document normal usage**

Add this section to `docs/operations/deployment.md` near the local binary/container deployment notes:

````markdown
### Updating the local Axon binary from GitHub Releases

Use `axon update` to install the latest published Linux release binary into
`~/.local/bin/axon` and restart the local Axon container against the same binary:

```bash
axon update
```

Useful variants:

```bash
axon update --version v5.9.2      # install a specific release tag
axon update --no-container        # update PATH only
axon update --force               # reinstall even if the version already matches
axon update --json                # machine-readable report
```

The command downloads `axon-linux-x86_64.tar.gz`, verifies
`axon-linux-x86_64.tar.gz.sha256`, installs atomically, and only syncs the
container after checksum and install succeed.
````

- [ ] **Step 2: Run format and focused tests**

Run:

```bash
cargo fmt --all
cargo test --locked update_
cargo test --locked update_defaults_to_latest_release_and_container_sync update_accepts_version_repo_no_container_and_force_flags
```

Expected: all pass.

- [ ] **Step 3: Run help smoke**

Run:

```bash
cargo run --locked --bin axon -- update --help
```

Expected: help includes `--repo`, `--version`, `--force`, and `--no-container`.

- [ ] **Step 4: Run dry local fake release smoke**

Run:

```bash
tmp="$(mktemp -d)"
printf '#!/usr/bin/env sh\necho axon fake-update\n' > "$tmp/axon"
chmod +x "$tmp/axon"
tar -C "$tmp" -czf "$tmp/axon-linux-x86_64.tar.gz" axon
sha256sum "$tmp/axon-linux-x86_64.tar.gz" > "$tmp/axon-linux-x86_64.tar.gz.sha256"
AXON_UPDATE_FILE_RELEASE_DIR="$tmp" \
AXON_UPDATE_INSTALL_PATH="$tmp/install/axon" \
cargo run --locked --bin axon -- update --version v0.0.0-test --no-container --force --json
"$tmp/install/axon"
```

Expected: JSON contains `"version":"v0.0.0-test"` and `"container_synced":false`; fake installed binary prints `axon fake-update`.

- [ ] **Step 5: Run real release install without container sync**

Run:

```bash
cargo run --locked --bin axon -- update --no-container --force
~/.local/bin/axon --version
```

Expected: downloads the latest GitHub Release, verifies checksum, installs to `~/.local/bin/axon`, and `~/.local/bin/axon --version` reports that release version.

- [ ] **Step 6: Run real release install with container sync**

Run only after Step 5 passes:

```bash
cargo run --locked --bin axon -- update --force
docker exec axon /home/axon/.axon/dev/axon --version
```

Expected: container command reports the same version as `~/.local/bin/axon --version`.

- [ ] **Step 7: Commit docs and verification polish**

Run:

```bash
git add docs/operations/deployment.md README.md
git commit -m "docs(update): document release binary updater"
```

## Final Quality Gate

- [ ] Run:

```bash
cargo fmt --all -- --check
cargo test --locked update_
cargo test --locked update_defaults_to_latest_release_and_container_sync update_accepts_version_repo_no_container_and_force_flags
cargo clippy --all-targets --locked -- -D warnings
```

- [ ] Run the real no-container updater once:

```bash
cargo run --locked --bin axon -- update --no-container --force
~/.local/bin/axon --version
```

- [ ] Run the real container sync proof once:

```bash
cargo run --locked --bin axon -- update --force
docker exec axon /home/axon/.axon/dev/axon --version
docker compose -f docker-compose.yaml ps axon
```

- [ ] Confirm the main checkout stayed untouched:

```bash
git -C /home/jmagar/workspace/axon status --short --branch
git -C /home/jmagar/workspace/axon/.worktrees/axon-update-release-sync status --short --branch
```

## Self-Review

- Spec coverage: The plan adds `axon update`, downloads GitHub Release assets, verifies the checksum, installs into PATH, and syncs the container to the installed binary.
- Placeholder scan: No task uses placeholder language; every code-changing step includes concrete snippets or exact command targets.
- Type consistency: `UpdateOptions`, `UpdateReport`, `ReleaseAssetNames`, `perform_update`, and helper names are defined before later tasks rely on them.
