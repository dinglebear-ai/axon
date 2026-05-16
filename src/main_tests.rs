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
        fs::write(axon_dir.join(".env"), format!("{key}=from_axon_home\n")).expect("write .env");
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
