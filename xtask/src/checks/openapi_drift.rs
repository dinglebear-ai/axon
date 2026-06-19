use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const GENERATED_ARTIFACTS: &[&str] = &[
    "apps/web/openapi/axon.json",
    "apps/web/lib/generated/axon-api.ts",
    "apps/palette-tauri/src/lib/axon-api.d.ts",
];

pub fn check(root: &Path) -> Result<()> {
    ensure_web_deps(root)?;
    export_openapi(root)?;
    run(
        root,
        "npm",
        &["--prefix", "apps/web", "run", "openapi:types"],
    )?;

    if !command_exists(root, "pnpm")? {
        bail!("pnpm is required to check Palette OpenAPI type drift");
    }

    ensure_palette_deps(root)?;
    run(
        root,
        "pnpm",
        &["--dir", "apps/palette-tauri", "generate:api"],
    )?;

    let drifted = generated_artifact_drift(root)?;
    if drifted.is_empty() {
        println!("OK: OpenAPI generated artifacts are in sync.");
        return Ok(());
    }

    eprintln!("ERROR: OpenAPI generated artifacts are out of date:");
    for path in &drifted {
        eprintln!("  {path}");
    }
    eprintln!();
    eprintln!("Run `cargo xtask check-openapi-drift` and commit the regenerated files.");
    bail!("OpenAPI generated artifact drift");
}

fn export_openapi(root: &Path) -> Result<()> {
    let output_path = root.join("apps/web/openapi/axon.json");
    let parent = output_path
        .parent()
        .context("OpenAPI output path must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let output = command_with_clean_env("cargo")
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            "Cargo.toml",
            "--bin",
            "axon-openapi",
        ])
        .current_dir(root)
        .output()
        .context(
            "failed to invoke `cargo run --quiet --manifest-path Cargo.toml --bin axon-openapi`",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`cargo run --quiet --manifest-path Cargo.toml --bin axon-openapi` failed with exit {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    fs::write(&output_path, output.stdout)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

fn ensure_web_deps(root: &Path) -> Result<()> {
    if root.join("apps/web/node_modules").is_dir() {
        return Ok(());
    }
    run(root, "npm", &["ci", "--prefix", "apps/web"])
}

fn ensure_palette_deps(root: &Path) -> Result<()> {
    if root.join("apps/palette-tauri/node_modules").is_dir() {
        return Ok(());
    }
    run(
        root,
        "pnpm",
        &[
            "--dir",
            "apps/palette-tauri",
            "install",
            "--frozen-lockfile",
        ],
    )
}

fn generated_artifact_drift(root: &Path) -> Result<Vec<String>> {
    let mut args = vec!["diff", "--name-only", "HEAD", "--"];
    args.extend(GENERATED_ARTIFACTS);
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("failed to invoke `git diff --name-only HEAD -- <openapi artifacts>`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`git diff --name-only HEAD -- <openapi artifacts>` failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("`git diff --name-only HEAD -- <openapi artifacts>` returned non-UTF-8 output")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn command_exists(root: &Path, program: &str) -> Result<bool> {
    let mut command = command_with_clean_env(program);
    let status = command
        .arg("--version")
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(status) => Ok(status.success()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to invoke `{program} --version`")),
    }
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let mut command = command_with_clean_env(program);
    let status = command
        .args(args)
        .current_dir(root)
        .status()
        .with_context(|| format!("failed to invoke `{}`", format_command(program, args)))?;

    if status.success() {
        return Ok(());
    }

    bail!(
        "`{}` failed with exit {}",
        format_command(program, args),
        status.code().unwrap_or(-1)
    );
}

fn command_with_clean_env(program: &str) -> Command {
    let mut command = Command::new(program);
    sanitize_cargo_env(&mut command);
    command
}

fn sanitize_cargo_env(command: &mut Command) {
    let cargo_env: Vec<OsString> = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| {
            let Some(key) = key.to_str() else {
                return false;
            };
            key == "CARGO"
                || key == "CARGO_MANIFEST_DIR"
                || key == "CARGO_BIN_NAME"
                || key == "CARGO_CRATE_NAME"
                || key == "CARGO_PRIMARY_PACKAGE"
                || key == "CARGO_TARGET_TMPDIR"
                || key == "CARGO_ENCODED_RUSTFLAGS"
                || key.starts_with("CARGO_CFG_")
                || key.starts_with("CARGO_PKG_")
                || key.starts_with("CARGO_PROFILE_")
        })
        .collect();

    for key in cargo_env {
        command.env_remove(key);
    }
    command.env("CARGO_UNSTABLE_CODEGEN_BACKEND", "1");
}

fn format_command(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}
