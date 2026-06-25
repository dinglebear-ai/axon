//! `axon palette` — resolve, launch, and optionally self-install the axon-palette desktop binary.
//!
//! The palette binary is the Tauri application in `apps/palette-tauri`. It builds as a standalone
//! workspace (`apps/palette-tauri/src-tauri` declares its own `[workspace]`) and is distributed as
//! a portable single binary (`axon-palette-tauri[.exe]`).
//!
//! Subcommands:
//!   axon palette                     — find and launch (auto-install prompt if missing)
//!   axon palette launch              — same as bare invocation
//!   axon palette install             — download release tarball matching this axon version
//!   axon palette install --method build  — build from source (requires pnpm + the Tauri toolchain)
//!   axon palette desktop             — write/refresh .desktop entry (Linux only)
//!   axon palette autostart           — write/refresh autostart entry (Linux only)

use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary};
use sha2::Digest;
use std::error::Error;
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[cfg(windows)]
const PALETTE_BIN: &str = "axon-palette-tauri.exe";
#[cfg(not(windows))]
const PALETTE_BIN: &str = "axon-palette-tauri";

pub async fn run_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let sub = cfg.positional.first().map(String::as_str);
    match sub {
        Some("install") => pull_or_build_palette(cfg).await,
        Some("launch") | None => launch_palette(cfg).await,
        Some("desktop") => write_desktop_entry_cmd(cfg),
        Some("autostart") => write_autostart_entry_cmd(cfg),
        Some(unknown) => Err(format!(
            "unknown palette subcommand '{unknown}'; valid: launch, install, desktop, autostart"
        )
        .into()),
    }
}

// ── Launch ────────────────────────────────────────────────────────────────────

async fn launch_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let exe = match resolve_palette_binary() {
        Some(p) => p,
        None => {
            if cfg.yes {
                // --yes acts as consent to auto-install
                pull_palette(cfg).await?;
                resolve_palette_binary()
                    .ok_or("palette binary not found after install; check PATH")?
            } else {
                print_not_found(cfg);
                return Err("axon-palette binary not found".into());
            }
        }
    };

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "status": "launching",
                "binary": exe.display().to_string(),
            }))?
        );
    } else {
        eprintln!(
            "{} {}",
            muted("launching"),
            accent(&exe.display().to_string())
        );
    }

    std::process::Command::new(&exe)
        .spawn()
        .map_err(|e| format!("failed to launch {}: {e}", exe.display()))?;
    Ok(())
}

fn print_not_found(cfg: &Config) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "error": "axon-palette binary not found",
                "hint": "run: axon palette install",
            }))
            .unwrap_or_default()
        );
    } else {
        eprintln!(
            "{} {}\n  {}",
            primary("axon-palette not found"),
            muted("— install with:"),
            accent("axon palette install"),
        );
    }
}

// ── Acquisition ───────────────────────────────────────────────────────────────

async fn pull_or_build_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.setup_method.as_deref() {
        Some("build") => build_palette(cfg),
        _ => pull_palette(cfg).await,
    }
}

async fn pull_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let (archive_url, sha_url) = palette_asset_urls();
    if !cfg.json_output {
        eprintln!(
            "{}",
            muted(&format!("downloading {PALETTE_BIN} from {archive_url}…"))
        );
    }

    let archive_bytes = download_verified(&archive_url, &sha_url).await?;
    let dest_dir = palette_install_dir()?;
    std::fs::create_dir_all(&dest_dir)?;
    extract_palette(&archive_bytes, &dest_dir)?;

    let dest = dest_dir.join(PALETTE_BIN);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    report_installed(cfg, &dest, "pull")
}

fn build_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if !cfg.json_output {
        eprintln!(
            "{}",
            muted("building axon-palette from source (may take several minutes)…")
        );
    }
    let palette_dir = find_palette_dir()?;

    // The Tauri palette needs its frontend deps and bundled assets before the
    // Rust binary can compile. `pnpm install` then `tauri build --no-bundle`
    // (which runs the frontend `beforeBuildCommand`) produces the portable
    // binary without packaging installers.
    run_in("pnpm", &["install", "--frozen-lockfile"], &palette_dir)?;
    run_in(
        "pnpm",
        &["exec", "tauri", "build", "--no-bundle", "--ci"],
        &palette_dir,
    )?;

    let built = palette_dir
        .join("src-tauri/target/release")
        .join(PALETTE_BIN);
    let dest_dir = palette_install_dir()?;
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(PALETTE_BIN);
    let tmp = dest_dir.join(format!(".{PALETTE_BIN}.tmp"));
    std::fs::copy(&built, &tmp)
        .map_err(|e| format!("copy {} → {}: {e}", built.display(), tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, &dest).map_err(|e| format!("rename into {}: {e}", dest.display()))?;
    report_installed(cfg, &dest, "build")
}

fn report_installed(cfg: &Config, dest: &Path, method: &str) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "status": "installed",
                "path": dest.display().to_string(),
                "method": method,
            }))?
        );
    } else {
        eprintln!(
            "{} {}",
            muted("installed →"),
            accent(&dest.display().to_string())
        );
    }
    Ok(())
}

// ── Download + verify ─────────────────────────────────────────────────────────

async fn download_verified(archive_url: &str, sha_url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let client = reqwest::Client::new();

    let sha_text = client
        .get(sha_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let expected = sha_text
        .split_whitespace()
        .next()
        .ok_or("sha256 file is empty")?
        .to_lowercase();

    let archive_bytes = client
        .get(archive_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let mut hasher = sha2::Sha256::new();
    hasher.update(&archive_bytes);
    let actual = hex::encode(hasher.finalize());

    if expected != actual {
        return Err(format!("checksum mismatch — expected {expected}, got {actual}").into());
    }

    Ok(archive_bytes.to_vec())
}

// ── Extraction ────────────────────────────────────────────────────────────────

fn extract_palette(archive_bytes: &Vec<u8>, dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    {
        extract_zip(archive_bytes, dest_dir)
    }
    #[cfg(not(windows))]
    {
        extract_targz(archive_bytes, dest_dir)
    }
}

#[cfg(not(windows))]
fn extract_targz(bytes: &Vec<u8>, dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    let gz = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name == PALETTE_BIN {
            entry.unpack(dest_dir.join(PALETTE_BIN))?;
            return Ok(());
        }
    }
    Err(format!("release archive did not contain {PALETTE_BIN}").into())
}

#[cfg(windows)]
fn extract_zip(bytes: &Vec<u8>, dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_owned();
        if std::path::Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            == Some(PALETTE_BIN)
        {
            let dest = dest_dir.join(PALETTE_BIN);
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut file, &mut out)?;
            return Ok(());
        }
    }
    Err(format!("release archive did not contain {PALETTE_BIN}").into())
}

// ── Resolution ────────────────────────────────────────────────────────────────

/// Find the palette binary: check the same dir as the running axon exe first
/// (co-installed), then walk PATH. Cross-platform, no `which` call needed.
fn resolve_palette_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(PALETTE_BIN);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(PALETTE_BIN);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Install next to the running axon binary so `resolve_palette_binary()` finds
/// it immediately on the next call. Falls back to `~/.local/bin` if the exe dir
/// is not writable or cannot be determined.
fn palette_install_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && !dir.as_os_str().is_empty()
    {
        return Ok(dir.to_path_buf());
    }
    Ok(expand_home("~/.local/bin"))
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        #[cfg(windows)]
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        #[cfg(not(windows))]
        let home = std::env::var_os("HOME");
        if let Some(h) = home {
            return PathBuf::from(h).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Run a command in `dir`, mapping a non-zero exit or spawn failure to an error.
fn run_in(program: &str, args: &[&str], dir: &Path) -> Result<(), Box<dyn Error>> {
    let status = std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("failed to run `{program}`: {e} (is it installed and on PATH?)"))?;
    if !status.success() {
        return Err(format!("`{program} {}` failed; see output above", args.join(" ")).into());
    }
    Ok(())
}

/// Walk up from the running binary to find the `apps/palette-tauri` source tree.
fn find_palette_dir() -> Result<PathBuf, Box<dyn Error>> {
    let start = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut cur = start.as_path();
    for _ in 0..8 {
        let dir = cur.join("apps/palette-tauri");
        if dir.join("src-tauri/Cargo.toml").is_file() {
            return Ok(dir);
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    let cwd = std::env::current_dir()?.join("apps/palette-tauri");
    if cwd.join("src-tauri/Cargo.toml").is_file() {
        return Ok(cwd);
    }
    Err("cannot find apps/palette-tauri; run from the axon repo root".into())
}

/// Version-matched, platform-aware asset URLs. Uses the running binary's own
/// version so `axon palette install` always fetches the matching palette release.
fn palette_asset_urls() -> (String, String) {
    let version = env!("CARGO_PKG_VERSION");
    #[cfg(windows)]
    let (target, ext) = ("windows-x86_64", "zip");
    #[cfg(not(windows))]
    let (target, ext) = ("linux-x86_64", "tar.gz");
    let base = format!(
        "https://github.com/jmagar/axon/releases/download/v{version}/axon-palette-{target}.{ext}"
    );
    let sha = format!("{base}.sha256");
    (base, sha)
}

// ── Desktop integration (Linux only) ─────────────────────────────────────────

fn write_desktop_entry_cmd(cfg: &Config) -> Result<(), Box<dyn Error>> {
    #[cfg(not(unix))]
    return Err("desktop integration is only supported on Linux".into());
    #[cfg(unix)]
    {
        let palette_path = resolve_palette_binary()
            .ok_or("axon-palette not found; install first with: axon palette install")?;
        let dest = expand_home("~/.local/share/applications/axon-palette.desktop");
        write_desktop_entry_at(&palette_path, &dest)?;
        report_path(cfg, "desktop_entry", &dest)
    }
}

fn write_autostart_entry_cmd(cfg: &Config) -> Result<(), Box<dyn Error>> {
    #[cfg(not(unix))]
    return Err("autostart integration is only supported on Linux".into());
    #[cfg(unix)]
    {
        let palette_path = resolve_palette_binary()
            .ok_or("axon-palette not found; install first with: axon palette install")?;
        let dest = expand_home("~/.config/autostart/axon-palette.desktop");
        write_desktop_entry_at(&palette_path, &dest)?;
        report_path(cfg, "autostart_entry", &dest)
    }
}

#[cfg(unix)]
fn write_desktop_entry_at(binary: &Path, dest: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!(
        "[Desktop Entry]\n\
         Name=Axon Palette\n\
         Comment=Global-hotkey command palette for axon\n\
         Exec={bin}\n\
         Icon=axon-palette\n\
         Type=Application\n\
         Categories=Utility;\n\
         StartupNotify=false\n",
        bin = binary.display()
    );
    std::fs::write(dest, content)?;
    Ok(())
}

fn report_path(cfg: &Config, key: &str, path: &Path) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({ "status": "ok", key: path.display().to_string() })
            )?
        );
    } else {
        eprintln!(
            "{} {}",
            muted(&format!("{key} →")),
            accent(&path.display().to_string())
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
