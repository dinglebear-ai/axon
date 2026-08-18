use super::*;
use crate::{SftpConnectionProfile, default_settings};

fn tempfile_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("axon-palette-{name}-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn base_settings() -> PaletteSettings {
    default_settings()
}

#[test]
fn write_settings_is_owner_only_when_sftp_connections_present() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile_dir("settings-sftp-perms");
    let path = dir.join("settings.json");

    let mut settings = base_settings();
    settings.sftp_connections = vec![SftpConnectionProfile {
        id: "abc".to_string(),
        label: "prod".to_string(),
        host: "example.com".to_string(),
        port: 22,
        username: "deploy".to_string(),
        private_key_path: "/home/me/.ssh/id_ed25519".to_string(),
    }];

    write_settings_to_path(&path, &settings).expect("write settings");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

/// `atomic_write` already creates every settings.json write at 0600
/// unconditionally (see its doc comment) — this asserts that baseline holds
/// even with no SFTP connections present, i.e. there is no separate
/// conditional-tightening path to regress. This replaces an earlier version
/// of this test that asserted permissions were "left at the default" (not
/// 0600) absent SFTP data; that assumption was wrong for this codebase and
/// the test failed against real behavior, which is what surfaced this.
#[test]
fn write_settings_is_owner_only_even_with_no_sftp_connections() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile_dir("settings-no-sftp-perms");
    let path = dir.join("settings.json");

    let settings = base_settings();
    assert!(settings.sftp_connections.is_empty());

    write_settings_to_path(&path, &settings).expect("write settings");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
