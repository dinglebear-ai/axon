use super::*;

#[test]
fn expand_home_replaces_home() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_home("~/foo/bar");
    assert_eq!(result, PathBuf::from(&home).join("foo/bar"));
}

#[test]
fn expand_home_no_tilde_unchanged() {
    let result = expand_home("/absolute/path");
    assert_eq!(result, PathBuf::from("/absolute/path"));
}

#[test]
fn expand_home_bare_tilde_returns_home() {
    // A path that starts with "~/" but has a trailing component.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_home("~/.local/bin");
    assert_eq!(result, PathBuf::from(&home).join(".local/bin"));
}

#[test]
fn palette_install_falls_back_when_executable_directory_is_not_writable() {
    let exe_dir = PathBuf::from("/usr/local/bin");
    let fallback = PathBuf::from("/home/test/.local/bin");

    assert_eq!(
        select_palette_install_dir(Some(exe_dir), fallback.clone(), |_| false),
        fallback
    );
}

#[test]
fn palette_install_prefers_writable_executable_directory() {
    let exe_dir = PathBuf::from("/opt/axon/bin");
    let fallback = PathBuf::from("/home/test/.local/bin");

    assert_eq!(
        select_palette_install_dir(Some(exe_dir.clone()), fallback, |_| true),
        exe_dir
    );
}

#[test]
fn find_palette_dir_finds_from_repo_root() {
    // This test only passes when run from inside the axon repo tree.
    // Skip when apps/palette-tauri isn't present (e.g. shallow CI clones).
    let cwd = std::env::current_dir().unwrap();
    let expected = cwd.join("apps/palette-tauri/src-tauri/Cargo.toml");
    if !expected.exists() {
        return; // Graceful skip.
    }
    let found = find_palette_dir().unwrap();
    assert!(
        found.join("src-tauri/Cargo.toml").is_file(),
        "should find src-tauri/Cargo.toml under {}",
        found.display()
    );
    assert!(
        found.ends_with("apps/palette-tauri"),
        "expected apps/palette-tauri, got {}",
        found.display()
    );
}

#[test]
fn write_desktop_entry_at_produces_valid_ini() {
    use std::io::Read;

    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("test.desktop");
    let binary = PathBuf::from("/usr/local/bin/axon-palette-tauri");

    write_desktop_entry_at(&binary, &dest).unwrap();

    let mut content = String::new();
    std::fs::File::open(&dest)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains("[Desktop Entry]"), "missing header");
    assert!(
        content.contains("Exec=/usr/local/bin/axon-palette-tauri"),
        "missing Exec line"
    );
    assert!(content.contains("Type=Application"), "missing Type");
}

#[test]
fn palette_release_selection_skips_assetless_latest_release() {
    let (archive_name, checksum_name) = palette_asset_names();
    let releases = vec![
        PaletteRelease {
            tag_name: "android-v99.0.0".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![],
        },
        PaletteRelease {
            tag_name: "palette-v5.14.2".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![
                PaletteAsset {
                    name: archive_name.to_string(),
                    browser_download_url: "https://example.test/palette".to_string(),
                },
                PaletteAsset {
                    name: checksum_name.to_string(),
                    browser_download_url: "https://example.test/palette.sha256".to_string(),
                },
            ],
        },
    ];

    assert_eq!(
        select_palette_assets(&releases),
        Some((
            "https://example.test/palette".to_string(),
            "https://example.test/palette.sha256".to_string()
        ))
    );
    assert!(PALETTE_RELEASES_API.contains("dinglebear-ai/axon"));
}

#[test]
fn palette_release_selection_requires_archive_and_checksum_from_same_release() {
    let (archive_name, checksum_name) = palette_asset_names();
    let releases = vec![
        PaletteRelease {
            tag_name: "palette-v5.14.2".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![PaletteAsset {
                name: archive_name.to_string(),
                browser_download_url: "https://example.test/palette".to_string(),
            }],
        },
        PaletteRelease {
            tag_name: "palette-v5.14.1".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![PaletteAsset {
                name: checksum_name.to_string(),
                browser_download_url: "https://example.test/palette.sha256".to_string(),
            }],
        },
    ];

    assert_eq!(select_palette_assets(&releases), None);
}

#[test]
fn palette_release_selection_skips_prereleases() {
    let (archive_name, checksum_name) = palette_asset_names();
    let release = |prerelease| PaletteRelease {
        tag_name: if prerelease {
            "palette-v5.15.0".to_string()
        } else {
            "palette-v5.14.2".to_string()
        },
        draft: false,
        prerelease,
        assets: vec![
            PaletteAsset {
                name: archive_name.to_string(),
                browser_download_url: format!("https://example.test/{prerelease}/palette"),
            },
            PaletteAsset {
                name: checksum_name.to_string(),
                browser_download_url: format!("https://example.test/{prerelease}/palette.sha256"),
            },
        ],
    };

    assert_eq!(
        select_palette_assets(&[release(true), release(false)]),
        Some((
            "https://example.test/false/palette".to_string(),
            "https://example.test/false/palette.sha256".to_string(),
        ))
    );
}

#[test]
fn palette_release_selection_uses_highest_semver_not_api_order() {
    let (archive_name, checksum_name) = palette_asset_names();
    let release = |tag: &str| PaletteRelease {
        tag_name: tag.to_string(),
        draft: false,
        prerelease: false,
        assets: vec![
            PaletteAsset {
                name: archive_name.to_string(),
                browser_download_url: format!("https://example.test/{tag}/palette"),
            },
            PaletteAsset {
                name: checksum_name.to_string(),
                browser_download_url: format!("https://example.test/{tag}/palette.sha256"),
            },
        ],
    };

    assert_eq!(
        select_palette_assets(&[
            release("palette-v5.13.9"),
            release("not-a-palette-version"),
            release("palette-v5.14.2"),
            release("palette-v5.14.1"),
        ]),
        Some((
            "https://example.test/palette-v5.14.2/palette".to_string(),
            "https://example.test/palette-v5.14.2/palette.sha256".to_string(),
        ))
    );
}

#[test]
fn palette_launch_rejects_an_immediate_child_failure() {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 23"])
        .spawn()
        .unwrap();

    let error = confirm_palette_started(&mut child, Path::new("test-palette"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("exited during startup"));
    assert!(error.contains("23"));
}

#[test]
fn palette_launch_accepts_a_running_child() {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "sleep 5"])
        .spawn()
        .unwrap();

    confirm_palette_started(&mut child, Path::new("test-palette")).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
}
