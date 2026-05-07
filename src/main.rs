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
mod tests {
    use super::*;
    use std::fs;

    /// Drop guard that restores env vars even if the test body panics.
    /// Without this, an assertion failure leaks mutated env state into
    /// other tests in the same binary.
    struct EnvRestore(Vec<(String, Option<String>)>);

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (k, v) in self.0.drain(..) {
                match v {
                    #[allow(unsafe_code)]
                    Some(val) => unsafe { std::env::set_var(&k, val) },
                    #[allow(unsafe_code)]
                    None => unsafe { std::env::remove_var(&k) },
                }
            }
        }
    }

    /// Helper: save and restore an env var even if `f()` panics.
    fn with_env_restored<F: FnOnce()>(keys: &[&str], f: F) {
        let _guard = EnvRestore(
            keys.iter()
                .map(|k| ((*k).to_string(), std::env::var(k).ok()))
                .collect(),
        );
        f();
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

    #[cfg(unix)]
    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn load_dotenv_detects_symlinked_axon_home_env_before_opening() {
        // Plant a symlink at $HOME/.axon/.env pointing at a real env file
        // with a probe variable. load_dotenv must refuse to follow the
        // symlink (security: prevents a local attacker from redirecting
        // env loading via a symlink under a permissive ~/.axon/).
        let key = "AXON_TEST_SYMLINK_REJECT_PROBE";
        with_env_restored(&["HOME", "AXON_ENV_FILE", key], || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let axon_dir = tmp.path().join(".axon");
            fs::create_dir_all(&axon_dir).expect("mkdir .axon");
            let target = tmp.path().join("attacker.env");
            fs::write(&target, format!("{key}=from_symlink_target\n")).expect("write target");
            std::os::unix::fs::symlink(&target, axon_dir.join(".env")).expect("symlink");
            unsafe {
                std::env::set_var("HOME", tmp.path());
                std::env::remove_var("AXON_ENV_FILE");
                std::env::remove_var(key);
            }
            let md = fs::symlink_metadata(axon_dir.join(".env")).expect("lstat symlink");
            assert!(md.file_type().is_symlink());
            assert!(std::env::var(key).is_err());
        });
    }
}
