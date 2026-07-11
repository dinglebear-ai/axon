use super::*;

/// Save + clear `AXON_DATA_DIR` so HOME-focused tests exercise the HOME branch
/// of `axon_home_dir()` (which now prefers `AXON_DATA_DIR` when set).
#[allow(unsafe_code)]
fn take_axon_data_dir() -> Option<String> {
    let saved = std::env::var("AXON_DATA_DIR").ok();
    unsafe { std::env::remove_var("AXON_DATA_DIR") };
    saved
}

#[allow(unsafe_code)]
fn restore_axon_data_dir(saved: Option<String>) {
    match saved {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_prefers_axon_data_dir_when_set() {
    let saved_home = std::env::var("HOME").ok();
    let saved_data = std::env::var("AXON_DATA_DIR").ok();
    unsafe {
        std::env::set_var("HOME", "/home/testuser");
        std::env::set_var("AXON_DATA_DIR", "/mnt/axon-data");
    }
    let home = axon_home_dir();
    let config = axon_config_path();
    match saved_home {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    // AXON_DATA_DIR is used verbatim (no `.axon` appended) and wins over HOME.
    assert_eq!(home, Some(PathBuf::from("/mnt/axon-data")));
    assert_eq!(config, Some(PathBuf::from("/mnt/axon-data/config.toml")));
}

#[cfg(unix)]
#[test]
fn ensure_private_dir_creates_with_0700_when_absent() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("secrets");
    ensure_private_dir(&target).expect("create");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "fresh dir should be 0o700");
}

#[cfg(unix)]
#[test]
fn ensure_private_dir_tightens_loose_mode_to_0700() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("loose");
    std::fs::create_dir(&target).expect("mkdir");
    std::fs::set_permissions(&target, PermissionsExt::from_mode(0o755)).expect("chmod 0755");
    ensure_private_dir(&target).expect("tighten");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "existing dir at 0o755 must be tightened to 0o700"
    );
}

#[cfg(unix)]
#[test]
fn ensure_private_dir_is_idempotent_when_already_0700() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("already");
    ensure_private_dir(&target).expect("first");
    ensure_private_dir(&target).expect("second");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[cfg(unix)]
#[tokio::test]
async fn ensure_private_dir_async_creates_with_0700_when_absent() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("async-secrets");
    ensure_private_dir_async(target.clone())
        .await
        .expect("create async");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn path_basename_extracts_filename() {
    assert_eq!(path_basename("/usr/bin/claude", "default"), "claude");
    assert_eq!(path_basename("simple", "default"), "simple");
}

#[test]
fn path_basename_uses_fallback_for_empty() {
    assert_eq!(path_basename("", "default"), "default");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_returns_some_when_home_set() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::set_var("HOME", "/home/testuser") };
    let result = axon_home_dir();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    let path = result.expect("axon_home_dir should return Some when HOME is set");
    assert!(path.to_string_lossy().ends_with(".axon"));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_returns_none_when_home_unset() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::remove_var("HOME") };
    let result = axon_home_dir();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    assert_eq!(result, None);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_returns_none_when_home_is_whitespace() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::set_var("HOME", "   ") };
    let result = axon_home_dir();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    assert_eq!(result, None);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_data_base_dir_uses_home_when_home_is_valid() {
    let saved_home = std::env::var("HOME").ok();
    let saved_data = std::env::var("AXON_DATA_DIR").ok();
    unsafe {
        std::env::remove_var("AXON_DATA_DIR");
        std::env::set_var("HOME", "/home/testuser");
    }
    let result = axon_data_base_dir();
    match saved_home {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match saved_data {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }
    assert_eq!(result, PathBuf::from("/home/testuser/.axon"));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_data_base_dir_does_not_fall_back_to_tmp_when_home_unset() {
    let saved_home = std::env::var("HOME").ok();
    let saved_data = std::env::var("AXON_DATA_DIR").ok();
    unsafe {
        std::env::remove_var("AXON_DATA_DIR");
        std::env::remove_var("HOME");
    }
    let result = axon_data_base_dir();
    match saved_home {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match saved_data {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }
    assert_eq!(result, PathBuf::from(".cache/axon-rust/data"));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_data_base_dir_rejects_relative_home() {
    let saved_home = std::env::var("HOME").ok();
    let saved_data = std::env::var("AXON_DATA_DIR").ok();
    unsafe {
        std::env::remove_var("AXON_DATA_DIR");
        std::env::set_var("HOME", "../relative/path");
    }
    let result = axon_data_base_dir();
    match saved_home {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match saved_data {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }
    assert_eq!(result, PathBuf::from(".cache/axon-rust/data"));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_data_base_dir_rejects_home_with_dotdot() {
    let saved_home = std::env::var("HOME").ok();
    let saved_data = std::env::var("AXON_DATA_DIR").ok();
    unsafe {
        std::env::remove_var("AXON_DATA_DIR");
        std::env::set_var("HOME", "/tmp/../etc");
    }
    let result = axon_data_base_dir();
    match saved_home {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match saved_data {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }
    assert_eq!(result, PathBuf::from(".cache/axon-rust/data"));
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_data_dir_accepts_windows_absolute_path() {
    let saved = std::env::var("AXON_DATA_DIR").ok();
    unsafe { std::env::set_var("AXON_DATA_DIR", r"C:\Users\jmaga\appdata\axon") };

    let direct = axon_data_dir();
    let base = axon_data_base_dir();

    match saved {
        Some(v) => unsafe { std::env::set_var("AXON_DATA_DIR", v) },
        None => unsafe { std::env::remove_var("AXON_DATA_DIR") },
    }

    let expected = PathBuf::from(r"C:\Users\jmaga\appdata\axon");
    assert_eq!(direct, Some(expected.clone()));
    assert_eq!(base, expected);
    assert!(
        base.is_absolute(),
        "drive-qualified Windows AXON_DATA_DIR must remain absolute"
    );
    assert_eq!(
        base.join("jobs.db"),
        PathBuf::from(r"C:\Users\jmaga\appdata\axon\jobs.db")
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_config_path_returns_none_when_home_unset() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::remove_var("HOME") };
    let result = axon_config_path();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    assert_eq!(result, None);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_returns_none_when_home_is_relative() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::set_var("HOME", "../relative/path") };
    let result = axon_home_dir();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    assert_eq!(
        result, None,
        "relative HOME should return None to prevent path traversal"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_home_dir_returns_none_when_home_contains_dotdot() {
    let saved = std::env::var("HOME").ok();
    let saved_data = take_axon_data_dir();
    unsafe { std::env::set_var("HOME", "/tmp/../etc") };
    let result = axon_home_dir();
    match saved {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    restore_axon_data_dir(saved_data);
    assert_eq!(
        result, None,
        "HOME containing .. should return None to prevent path traversal"
    );
}
