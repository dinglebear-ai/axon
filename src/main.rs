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
                    "warning: refusing to load symlinked .env at {} (potential symlink attack)",
                    home_env.display()
                );
            }
            Ok(_) => match dotenvy::from_path(&home_env) {
                Ok(_) => return,
                Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(dotenvy::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::PermissionDenied =>
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
    // ACP sessions consume one spawn_blocking thread each for up to 300s (ACP_ADAPTER_TIMEOUT).
    // max_blocking_threads caps the blocking thread pool to prevent silent exhaustion that
    // would starve DB queries and file I/O. Logical ACP session concurrency is controlled
    // separately by AXON_ACP_MAX_CONCURRENT_SESSIONS (default 8) — tune that env var to
    // limit simultaneous ACP sessions. AXON_MAX_BLOCKING_THREADS only caps the Tokio
    // blocking thread pool capacity; set it high enough to serve blocking-thread consumers
    // (ACP sessions, file I/O, DB) without exhaustion.
    // See: docs/reports/acp-performance-scalability-analysis-2026-03-08.md FINDING-6
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(acp_blocking_thread_limit())
        // ACP session setup future exceeds Tokio's default 2 MB worker stack on debug builds —
        // we hit "thread '<unnamed>' has overflowed its stack" during ACP init before this
        // bump. Memory cost: 8 MB × `tokio::runtime` worker count (default = num_cpus). On a
        // 16-core homelab box that's ~128 MB virtual reservation per process; the OS only
        // commits pages actually touched, so resident set growth is much smaller.
        .thread_stack_size(8 * 1024 * 1024)
        .build()
        .expect("failed to build tokio runtime");
    rt.block_on(async_main())
}

fn acp_blocking_thread_limit() -> usize {
    // Default: 64 blocking threads for ACP + other blocking work (file I/O, DB).
    // This caps Tokio's blocking thread pool — NOT the logical ACP session limit.
    // Tune AXON_ACP_MAX_CONCURRENT_SESSIONS (default 8) to control how many ACP
    // sessions run simultaneously. Tune AXON_MAX_BLOCKING_THREADS to size the
    // blocking thread pool for all blocking consumers. For homelab single-user use,
    // 16–32 blocking threads is typically sufficient.
    std::env::var("AXON_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0) // reject 0 — tokio::Builder::max_blocking_threads panics on 0
        .unwrap_or(64)
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
mod tests {
    use super::*;
    use std::fs;

    /// Helper: save and restore an env var unconditionally.
    fn with_env_restored<F: FnOnce()>(keys: &[&str], f: F) {
        let saved: Vec<(String, Option<String>)> = keys
            .iter()
            .map(|k| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        f();
        for (k, v) in saved {
            match v {
                #[allow(unsafe_code)]
                Some(val) => unsafe { std::env::set_var(&k, val) },
                #[allow(unsafe_code)]
                None => unsafe { std::env::remove_var(&k) },
            }
        }
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn load_dotenv_loads_axon_home_env_when_present() {
        let key = "AXON_TEST_HOME_ENV_VAR_LOADED";
        with_env_restored(&["HOME", "AXON_ENV_FILE", key], || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let axon_dir = tmp.path().join(".axon");
            fs::create_dir_all(&axon_dir).expect("mkdir .axon");
            fs::write(axon_dir.join(".env"), format!("{key}=from_axon_home\n"))
                .expect("write .env");
            unsafe {
                std::env::set_var("HOME", tmp.path());
                std::env::remove_var("AXON_ENV_FILE");
                std::env::remove_var(key);
            }
            load_dotenv();
            assert_eq!(std::env::var(key).ok().as_deref(), Some("from_axon_home"));
        });
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn load_dotenv_falls_through_when_axon_home_env_absent() {
        // ~/.axon/.env missing → function should not panic and should not set our
        // probe key. (Verifying repo-root .env loading is environment-dependent;
        // the contract we care about is "no early return / no panic".)
        let key = "AXON_TEST_FALLTHROUGH_PROBE_VAR";
        with_env_restored(&["HOME", "AXON_ENV_FILE", key], || {
            let tmp = tempfile::tempdir().expect("tempdir");
            // Note: no .axon/.env created
            unsafe {
                std::env::set_var("HOME", tmp.path());
                std::env::remove_var("AXON_ENV_FILE");
                std::env::remove_var(key);
            }
            load_dotenv();
            // Probe key was never written anywhere → must remain unset.
            assert!(std::env::var(key).is_err());
        });
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn load_dotenv_axon_env_file_wins_over_axon_home() {
        let key = "AXON_TEST_PRECEDENCE_VAR";
        with_env_restored(&["HOME", "AXON_ENV_FILE", key], || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let axon_dir = tmp.path().join(".axon");
            fs::create_dir_all(&axon_dir).expect("mkdir .axon");
            fs::write(axon_dir.join(".env"), format!("{key}=from_axon_home\n"))
                .expect("write home .env");
            let explicit = tmp.path().join("explicit.env");
            fs::write(&explicit, format!("{key}=from_explicit\n")).expect("write explicit env");
            unsafe {
                std::env::set_var("HOME", tmp.path());
                std::env::set_var("AXON_ENV_FILE", &explicit);
                std::env::remove_var(key);
            }
            load_dotenv();
            assert_eq!(std::env::var(key).ok().as_deref(), Some("from_explicit"));
        });
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn load_dotenv_silent_fall_through_when_home_unset() {
        let key = "AXON_TEST_HOME_UNSET_PROBE";
        with_env_restored(&["HOME", "AXON_ENV_FILE", key], || {
            unsafe {
                std::env::remove_var("HOME");
                std::env::remove_var("AXON_ENV_FILE");
                std::env::remove_var(key);
            }
            // Should not panic.
            load_dotenv();
            assert!(std::env::var(key).is_err());
        });
    }
}
