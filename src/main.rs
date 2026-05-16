#![recursion_limit = "512"]
use std::path::PathBuf;

fn find_dotenv_from_launch_context() -> Option<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.push(parent.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    for root in roots {
        for dir in root.ancestors() {
            let candidate = dir.join(".env");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn load_dotenv() {
    if let Some(explicit) = std::env::var_os("AXON_ENV_FILE").map(PathBuf::from) {
        match dotenvy::from_path(&explicit) {
            Ok(_) => return,
            Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!(
                    "warning: failed to load AXON_ENV_FILE ({}): {e}",
                    explicit.display()
                );
            }
        }
    }

    if let Some(home_env) = axon::core::paths::axon_home_dir().map(|d| d.join(".env")) {
        // Reject symlinks under ~/.axon/ — this directory holds secrets and
        // we do not want a planted symlink redirecting us to attacker-controlled
        // env. Bare `dotenvy::from_path` follows symlinks via `File::open`.
        match std::fs::symlink_metadata(&home_env) {
            Ok(md) if md.file_type().is_symlink() => {
                eprintln!(
                    "error: refusing to load symlinked .env at {} (potential symlink attack); refusing to fall through to repo-root .env to avoid masking production secrets",
                    home_env.display()
                );
                std::process::exit(1);
            }
            Ok(_) => match dotenvy::from_path(&home_env) {
                Ok(_) => return,
                Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(dotenvy::Error::Io(ref e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::IsADirectory
                            | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    eprintln!(
                        "error: cannot read {} ({e}); refusing to fall through to repo-root .env to avoid masking production secrets",
                        home_env.display()
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to load .env from {}: {e}",
                        home_env.display()
                    );
                }
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::IsADirectory
                        | std::io::ErrorKind::NotADirectory
                ) =>
            {
                eprintln!(
                    "error: cannot stat .env at {} ({e}); refusing to fall through to repo-root .env to avoid masking production secrets",
                    home_env.display()
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!(
                    "warning: failed to stat .env at {}: {e}",
                    home_env.display()
                );
            }
        }
    }

    if let Some(path) = find_dotenv_from_launch_context() {
        match dotenvy::from_path(&path) {
            Ok(_) => return,
            Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!("warning: failed to load .env from {}: {e}", path.display());
                return;
            }
        }
    }

    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            eprintln!("warning: failed to load .env: {e}");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()
        .expect("failed to build tokio runtime");
    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Install aws-lc-rs as the process-level rustls crypto provider before any
    // TLS connections are made. Both ring (via lapin) and aws-lc-rs (via octocrab /
    // spider / reqwest 0.12) are compiled into the same binary, so rustls 0.23
    // cannot auto-select one and panics without this call. Returns Err if already
    // installed (e.g. in tests) — safe to ignore.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    load_dotenv();

    axon::run().await
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
