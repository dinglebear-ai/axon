use super::*;
use crate::{SftpConnectionProfile, default_settings};

fn tempfile_dir(name: &str) -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("axon-palette-{name}-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn base_settings() -> PaletteSettings {
    default_settings(&[])
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

#[test]
fn trim_env_value_handles_escape_edge_cases() {
    // Unknown escape sequence: \n is not recognised — backslash + 'n' pass through
    // r#""value\nraw""# is the 12-char string: "value\nraw"
    assert_eq!(trim_env_value(r#""value\nraw""#), r"value\nraw");
    // Terminal lone backslash: r#""a\""# is the 4-char string "a\"
    // outer quotes are stripped, inner "a\" → unescape: 'a' then '\' then EOF → "a\"
    assert_eq!(trim_env_value(r#""a\""#), "a\\");
    // \" inside double-quoted value is expanded to a literal double-quote
    // r#""say\"hi\"""# is: "say\"hi\""
    assert_eq!(trim_env_value(r#""say\"hi\"""#), r#"say"hi""#);
    // \\ inside double-quoted value is expanded to a single backslash
    // r#""one\\two""# is: "one\\two"
    assert_eq!(trim_env_value(r#""one\\two""#), r"one\two");
}

#[test]
fn format_trim_env_value_roundtrip_with_special_characters() {
    for raw in [
        "simple",
        "with spaces",
        "with#hash",
        "with$dollar",
        r#"with"quotes"#,
        "with'single",
        r"with\backslash",
        "",
    ] {
        let formatted = format_env_value(raw);
        let recovered = trim_env_value(&formatted);
        assert_eq!(
            recovered, raw,
            "round-trip failed for {raw:?}: formatted={formatted:?} recovered={recovered:?}"
        );
    }
}
