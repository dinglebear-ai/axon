//! `axon palette` — resolve, launch, and optionally install the axon-palette desktop binary.
//!
//! The palette binary (`axon-palette`) is a SEPARATE GPUI application (`apps/desktop`) with a
//! conflicting dependency tree (wasm-bindgen 0.2.120+ via gpui_wgpu conflicts with reqwest-locked
//! 0.2.118). It must NOT be merged into this workspace.
//!
//! Subcommands (via positional arg):
//!   axon palette           — resolve and launch (install .desktop on first run)
//!   axon palette launch    — same as bare invocation
//!   axon palette install   — acquire binary (requires --method pull|build)
//!   axon palette desktop   — write/refresh ~/.local/share/applications/axon-palette.desktop
//!   axon palette autostart — write/refresh ~/.config/autostart/axon-palette.desktop
//!
//! Acquisition methods (--method):
//!   pull  — download release tarball from GitHub releases
//!   build — cargo build --manifest-path apps/desktop/Cargo.toml

use crate::core::config::Config;
use crate::core::ui::{accent, muted, primary};
use std::error::Error;
use std::path::{Path, PathBuf};

const PALETTE_BIN: &str = "axon-palette";
const RELEASE_TARBALL_URL: &str =
    "https://github.com/jmagar/axon/releases/latest/download/axon-palette-linux-x86_64.tar.gz";
const DESKTOP_ENTRY_PATH: &str = "~/.local/share/applications/axon-palette.desktop";
const AUTOSTART_PATH: &str = "~/.config/autostart/axon-palette.desktop";

/// Search directories probed in priority order when locating the palette binary.
const PALETTE_SEARCH_DIRS: &[&str] = &["~/.local/bin", "~/.axon/bin"];

pub async fn run_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let sub = cfg.positional.first().map(String::as_str);
    match sub {
        Some("desktop") => write_desktop_entry_cmd(cfg),
        Some("autostart") => write_autostart_entry_cmd(cfg),
        Some("install") => acquire_palette(cfg),
        Some("launch") | None => launch_palette(cfg),
        Some(unknown) => Err(format!(
            "unknown palette subcommand '{unknown}'; valid: launch, install, desktop, autostart"
        )
        .into()),
    }
}

// ─── Launch ──────────────────────────────────────────────────────────────────

fn launch_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let palette_path = match resolve_palette_binary() {
        Some(p) => p,
        None => {
            // Try acquisition when --method was supplied alongside the bare command.
            if cfg.setup_method.is_some() {
                acquire_palette(cfg)?;
                resolve_palette_binary()
                    .ok_or("palette binary not found after install; check $PATH or ~/.axon/bin")?
            } else {
                print_not_found(cfg);
                return Err("axon-palette binary not found".into());
            }
        }
    };

    // Silently write a .desktop entry on the very first launch.
    let desktop = expand_tilde(DESKTOP_ENTRY_PATH);
    if !desktop.exists() {
        let _ = write_desktop_entry_at(&palette_path, &desktop);
    }

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "status": "launching",
                "binary": palette_path.display().to_string(),
            }))?
        );
    } else {
        eprintln!(
            "{} {}",
            muted("launching"),
            accent(&palette_path.display().to_string())
        );
    }

    std::process::Command::new(&palette_path)
        .spawn()
        .map_err(|e| -> Box<dyn Error> {
            format!("failed to launch {}: {e}", palette_path.display()).into()
        })?;

    Ok(())
}

fn print_not_found(cfg: &Config) {
    if cfg.json_output {
        let msg = serde_json::json!({
            "error": "axon-palette binary not found",
            "hint": "acquire with: axon palette install --method pull|build",
        });
        println!("{}", serde_json::to_string(&msg).unwrap_or_default());
    } else {
        eprintln!(
            "{} {}\n  {}\n  {}",
            primary("axon-palette not found"),
            muted("— acquire with one of:"),
            accent("axon palette install --method pull"),
            accent("axon palette install --method build"),
        );
    }
}

// ─── Acquisition ─────────────────────────────────────────────────────────────

fn acquire_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.setup_method.as_deref() {
        Some("pull") => pull_palette(cfg),
        Some("build") => build_palette(cfg),
        _ => Err("--method pull or --method build is required for palette install".into()),
    }
}

fn pull_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if !cfg.json_output {
        eprintln!(
            "{}",
            muted(&format!("downloading {PALETTE_BIN} from GitHub releases…"))
        );
    }

    let dest_dir = expand_tilde("~/.local/bin");
    std::fs::create_dir_all(&dest_dir)?;

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "curl -fsSL '{url}' | tar -xzf - -C '{dir}' {bin} 2>&1",
            url = RELEASE_TARBALL_URL,
            dir = dest_dir.display(),
            bin = PALETTE_BIN,
        ))
        .status()
        .map_err(|e| format!("curl/tar failed: {e}"))?;

    if !status.success() {
        return Err(format!(
            "failed to download {PALETTE_BIN}; check network and release at {RELEASE_TARBALL_URL}"
        )
        .into());
    }

    let dest = make_executable(dest_dir.join(PALETTE_BIN))?;
    report_installed(cfg, &dest, "pull")
}

fn build_palette(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if !cfg.json_output {
        eprintln!(
            "{}",
            muted("building axon-palette from source (this may take a few minutes)…")
        );
    }

    let manifest_path = find_desktop_manifest()?;

    let status = std::process::Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(&manifest_path)
        .status()
        .map_err(|e| format!("cargo build failed: {e}"))?;

    if !status.success() {
        return Err("cargo build of axon-palette failed; see output above".into());
    }

    let built = manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("target/release")
        .join(PALETTE_BIN);

    let dest_dir = expand_tilde("~/.local/bin");
    std::fs::create_dir_all(&dest_dir)?;
    let dest_path = dest_dir.join(PALETTE_BIN);

    // Atomic copy: write to a temp file, make executable, rename into place.
    let tmp = dest_dir.join(format!(".{PALETTE_BIN}.tmp"));
    std::fs::copy(&built, &tmp)
        .map_err(|e| format!("copy {} → {}: {e}", built.display(), tmp.display()))?;
    let dest = make_executable(tmp)?;
    std::fs::rename(&dest, &dest_path)
        .map_err(|e| format!("rename into {}: {e}", dest_path.display()))?;

    report_installed(cfg, &dest_path, "build")
}

fn make_executable(path: PathBuf) -> Result<PathBuf, Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(path)
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

// ─── Desktop integration ─────────────────────────────────────────────────────

fn write_desktop_entry_cmd(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let palette_path = resolve_palette_binary()
        .ok_or("axon-palette not found; install first with: axon palette install")?;
    let dest = expand_tilde(DESKTOP_ENTRY_PATH);
    write_desktop_entry_at(&palette_path, &dest)?;
    report_path(cfg, "desktop_entry", &dest)
}

fn write_autostart_entry_cmd(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let palette_path = resolve_palette_binary()
        .ok_or("axon-palette not found; install first with: axon palette install")?;
    let dest = expand_tilde(AUTOSTART_PATH);
    write_desktop_entry_at(&palette_path, &dest)?;
    report_path(cfg, "autostart_entry", &dest)
}

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
            serde_json::to_string(&serde_json::json!({
                "status": "ok",
                key: path.display().to_string(),
            }))?
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

// ─── Resolution helpers ───────────────────────────────────────────────────────

/// Probe `which axon-palette`, then fall back to known install directories.
fn resolve_palette_binary() -> Option<PathBuf> {
    // 1. `which` covers the user's current PATH (handles rbenv/mise shims, etc.).
    if let Ok(out) = std::process::Command::new("which")
        .arg(PALETTE_BIN)
        .output()
        && out.status.success()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    // 2. Known directories not always on PATH (e.g. first-run before shell reload).
    for dir in PALETTE_SEARCH_DIRS {
        let candidate = expand_tilde(dir).join(PALETTE_BIN);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Walk up from the running binary to find `apps/desktop/Cargo.toml`.
fn find_desktop_manifest() -> Result<PathBuf, Box<dyn Error>> {
    let search_root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let mut candidate = search_root.as_path();
    for _ in 0..8 {
        let manifest = candidate.join("apps/desktop/Cargo.toml");
        if manifest.is_file() {
            return Ok(manifest);
        }
        match candidate.parent() {
            Some(p) => candidate = p,
            None => break,
        }
    }

    // Final fallback: current working directory.
    let cwd_manifest = std::env::current_dir()?.join("apps/desktop/Cargo.toml");
    if cwd_manifest.is_file() {
        return Ok(cwd_manifest);
    }

    Err("cannot find apps/desktop/Cargo.toml; run from the axon repo root or a git worktree".into())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
