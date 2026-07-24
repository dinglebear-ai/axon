use super::*;
use std::fs;
use std::path::PathBuf;

fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(name);
    fs::write(&path, contents).expect("write file");
    (dir, path)
}

#[test]
fn parses_inline_array() {
    let (_tmp, path) = write_tmp(
        "audit.toml",
        "[advisories]\nignore = [\"RUSTSEC-2023-0071\", \"RUSTSEC-2026-0183\"]\n",
    );
    let ids = parse_ignore_list(&path).expect("parse");
    assert_eq!(ids, vec!["RUSTSEC-2023-0071", "RUSTSEC-2026-0183"]);
}

#[test]
fn parses_multiline_array_with_comments() {
    // Matches deny.toml's canonical annotated layout.
    let (_tmp, path) = write_tmp(
        "deny.toml",
        "[advisories]\nignore = [\n  # rsa Marvin attack\n  \"RUSTSEC-2023-0071\",\n  # git2 unsound\n  \"RUSTSEC-2026-0183\",\n]\n",
    );
    let ids = parse_ignore_list(&path).expect("parse");
    assert_eq!(ids, vec!["RUSTSEC-2023-0071", "RUSTSEC-2026-0183"]);
}

#[test]
fn ignores_non_rustsec_ids() {
    let (_tmp, path) = write_tmp(
        "audit.toml",
        "[advisories]\nignore = [\"RUSTSEC-2023-0071\", \"other-id\"]\n",
    );
    let ids = parse_ignore_list(&path).expect("parse");
    assert_eq!(ids, vec!["RUSTSEC-2023-0071"]);
}

#[test]
fn check_passes_when_lists_match() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cargo_dir = dir.path().join(".cargo");
    fs::create_dir_all(&cargo_dir).expect("mkdir");
    let body = "[advisories]\nignore = [\"RUSTSEC-2023-0071\"]\n";
    fs::write(cargo_dir.join("audit.toml"), body).expect("write audit");
    fs::write(dir.path().join("deny.toml"), body).expect("write deny");
    check(dir.path()).expect("in-sync lists should pass");
}

#[test]
fn check_fails_with_drift() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cargo_dir = dir.path().join(".cargo");
    fs::create_dir_all(&cargo_dir).expect("mkdir");
    fs::write(
        cargo_dir.join("audit.toml"),
        "[advisories]\nignore = [\"RUSTSEC-2023-0071\"]\n",
    )
    .expect("write audit");
    fs::write(
        dir.path().join("deny.toml"),
        "[advisories]\nignore = [\"RUSTSEC-2026-0183\"]\n",
    )
    .expect("write deny");
    let err = check(dir.path()).expect_err("drifted lists should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("drift"), "error should mention drift: {msg}");
}

#[test]
fn check_passes_when_both_empty() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cargo_dir = dir.path().join(".cargo");
    fs::create_dir_all(&cargo_dir).expect("mkdir");
    fs::write(cargo_dir.join("audit.toml"), "[advisories]\nignore = []\n").expect("write audit");
    fs::write(dir.path().join("deny.toml"), "[advisories]\nignore = []\n").expect("write deny");
    check(dir.path()).expect("empty lists should pass");
}
