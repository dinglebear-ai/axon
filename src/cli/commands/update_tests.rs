use super::*;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::Sha256;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tar::{Builder, Header};

#[test]
fn linux_x86_64_release_asset_names_are_expected() {
    let names = release_asset_names("linux", "x86_64").unwrap();

    assert_eq!(names.archive, "axon-linux-x86_64.tar.gz");
    assert_eq!(names.checksum, "axon-linux-x86_64.tar.gz.sha256");
}

#[test]
fn unsupported_platform_returns_clear_error() {
    let err = release_asset_names("darwin", "aarch64").unwrap_err();

    assert!(err.to_string().contains("unsupported platform"));
    assert!(err.to_string().contains("darwin/aarch64"));
}

#[test]
fn parses_sha256_sidecar_with_filename() {
    let parsed = parse_sha256_sidecar(
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08  axon-linux-x86_64.tar.gz\n",
    )
    .unwrap();

    assert_eq!(
        parsed,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
}

#[test]
fn checksum_mismatch_is_rejected() {
    let err = verify_sha256(
        b"test",
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap_err();

    assert!(err.to_string().contains("checksum mismatch"));
}

#[test]
fn checksum_match_is_accepted() {
    verify_sha256(
        b"test",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    )
    .unwrap();
}

fn make_release_archive(script_body: &str) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = Builder::new(&mut tar_bytes);
        let mut header = Header::new_gnu();
        header.set_path("axon").unwrap();
        header.set_size(script_body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append(&header, script_body.as_bytes())
            .expect("append axon");
        builder.finish().expect("finish tar");
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn extracts_axon_binary_from_release_archive() {
    let archive = make_release_archive("#!/usr/bin/env sh\necho axon 5.9.2\n");
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("axon-linux-x86_64.tar.gz");
    fs::write(&archive_path, archive).unwrap();
    let extracted = extract_axon_binary(&archive_path, temp.path()).unwrap();

    assert_eq!(
        fs::read_to_string(&extracted).unwrap(),
        "#!/usr/bin/env sh\necho axon 5.9.2\n"
    );
}

#[test]
fn atomic_install_replaces_destination_and_sets_executable_mode() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("axon-new");
    let dest = temp.path().join("bin").join("axon");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&source, "#!/usr/bin/env sh\necho new\n").unwrap();
    fs::write(&dest, "#!/usr/bin/env sh\necho old\n").unwrap();

    install_binary_atomically(&source, &dest).unwrap();

    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "#!/usr/bin/env sh\necho new\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&dest).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}

#[tokio::test]
async fn update_installs_from_file_release_dir_without_container_sync() {
    let temp = tempfile::tempdir().unwrap();
    let release = make_release_archive("#!/usr/bin/env sh\necho axon 5.9.2\n");
    let checksum = hex::encode(Sha256::digest(&release));
    let archive_path = temp.path().join("axon-linux-x86_64.tar.gz");
    let checksum_path = temp.path().join("axon-linux-x86_64.tar.gz.sha256");
    fs::write(&archive_path, &release).unwrap();
    fs::write(
        &checksum_path,
        format!("{checksum}  axon-linux-x86_64.tar.gz\n"),
    )
    .unwrap();

    let install_dir = temp.path().join("install");
    let options = UpdateOptions {
        repo: "jmagar/axon".to_string(),
        version: Some("v5.9.2".to_string()),
        force: true,
        sync_container: false,
        install_path: install_dir.join("axon"),
        file_release_dir: Some(temp.path().to_path_buf()),
    };

    let report = perform_update(options).await.unwrap();

    assert_eq!(report.version, "v5.9.2");
    assert!(report.installed);
    assert_eq!(
        fs::read_to_string(install_dir.join("axon")).unwrap(),
        "#!/usr/bin/env sh\necho axon 5.9.2\n"
    );
    assert!(!report.container_synced);
}

#[tokio::test]
async fn update_skips_install_when_existing_binary_reports_target_version() {
    let temp = tempfile::tempdir().unwrap();
    let release = make_release_archive("#!/usr/bin/env sh\necho replacement\n");
    let checksum = hex::encode(Sha256::digest(&release));
    fs::write(temp.path().join("axon-linux-x86_64.tar.gz"), &release).unwrap();
    fs::write(
        temp.path().join("axon-linux-x86_64.tar.gz.sha256"),
        format!("{checksum}  axon-linux-x86_64.tar.gz\n"),
    )
    .unwrap();

    let install_path = temp.path().join("install").join("axon");
    fs::create_dir_all(install_path.parent().unwrap()).unwrap();
    fs::write(&install_path, "#!/usr/bin/env sh\necho axon 5.9.2\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&install_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&install_path, permissions).unwrap();
    }

    let report = perform_update(UpdateOptions {
        repo: "jmagar/axon".to_string(),
        version: Some("v5.9.2".to_string()),
        force: false,
        sync_container: false,
        install_path: install_path.clone(),
        file_release_dir: Some(temp.path().to_path_buf()),
    })
    .await
    .unwrap();

    assert!(!report.installed);
    assert_eq!(
        fs::read_to_string(install_path).unwrap(),
        "#!/usr/bin/env sh\necho axon 5.9.2\n"
    );
}

#[test]
fn version_output_requires_exact_normalized_token_match() {
    assert!(output_reports_version("axon 5.9.2\n", "v5.9.2"));
    assert!(output_reports_version("axon v5.9.2\n", "5.9.2"));
    assert!(!output_reports_version("axon 5.9.20\n", "v5.9.2"));
    assert!(!output_reports_version("axon 15.9.2\n", "v5.9.2"));
}

#[tokio::test]
async fn update_replaces_binary_when_existing_version_is_only_a_prefix_match() {
    let temp = tempfile::tempdir().unwrap();
    let release = make_release_archive("#!/usr/bin/env sh\necho axon 5.9.2\n");
    let checksum = hex::encode(Sha256::digest(&release));
    fs::write(temp.path().join("axon-linux-x86_64.tar.gz"), &release).unwrap();
    fs::write(
        temp.path().join("axon-linux-x86_64.tar.gz.sha256"),
        format!("{checksum}  axon-linux-x86_64.tar.gz\n"),
    )
    .unwrap();

    let install_path = temp.path().join("install").join("axon");
    fs::create_dir_all(install_path.parent().unwrap()).unwrap();
    fs::write(&install_path, "#!/usr/bin/env sh\necho axon 5.9.20\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&install_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&install_path, permissions).unwrap();
    }

    let report = perform_update(UpdateOptions {
        repo: "jmagar/axon".to_string(),
        version: Some("v5.9.2".to_string()),
        force: false,
        sync_container: false,
        install_path: install_path.clone(),
        file_release_dir: Some(temp.path().to_path_buf()),
    })
    .await
    .unwrap();

    assert!(report.installed);
    assert_eq!(
        fs::read_to_string(install_path).unwrap(),
        "#!/usr/bin/env sh\necho axon 5.9.2\n"
    );
}

#[test]
fn sync_container_uses_installed_binary_directory_as_dev_target() {
    let temp = tempfile::tempdir().unwrap();
    let compose_dir = temp.path().join(".axon").join("compose");
    let compose_file = compose_dir.join("docker-compose.yaml");
    let env_file = temp.path().join(".axon").join(".env");
    fs::create_dir_all(&compose_dir).unwrap();
    fs::write(&compose_file, "services: {}\n").unwrap();
    fs::write(&env_file, "TEST_KEY=1\n").unwrap();

    let fake_bin = temp.path().join("bin").join("axon");
    fs::create_dir_all(fake_bin.parent().unwrap()).unwrap();
    fs::write(&fake_bin, "#!/usr/bin/env sh\necho axon 5.9.2\n").unwrap();

    let sync = build_container_sync_command_with_paths(
        &fake_bin,
        ComposePaths {
            compose_dir: compose_dir.clone(),
            compose_file: compose_file.clone(),
            env_file: Some(env_file.clone()),
        },
    )
    .unwrap();

    assert_eq!(sync.env_name, "AXON_DEV_TARGET_DIR");
    assert_eq!(sync.env_value, fake_bin.parent().unwrap());
    assert_eq!(sync.program, "docker");
    assert_eq!(sync.current_dir, compose_dir);
    assert_eq!(sync.args.first().map(String::as_str), Some("compose"));
    assert!(sync.args.windows(2).any(|args| {
        args == [
            "-f",
            &sync
                .current_dir
                .join("docker-compose.yaml")
                .display()
                .to_string(),
        ]
    }));
    assert!(sync.args.ends_with(&[
        "up".to_string(),
        "-d".to_string(),
        "axon".to_string(),
        "--no-deps".to_string(),
        "--no-build".to_string(),
        "--force-recreate".to_string(),
    ]));
}

#[test]
fn env_file_args_are_inserted_before_absolute_compose_file() {
    let paths = ComposePaths {
        compose_dir: PathBuf::from("/home/j/.axon/compose"),
        compose_file: PathBuf::from("/home/j/.axon/compose/docker-compose.yaml"),
        env_file: Some(PathBuf::from("/home/j/.axon/.env")),
    };
    let args = compose_args(&paths, true);

    assert_eq!(
        args,
        vec![
            "compose",
            "--env-file",
            "/home/j/.axon/.env",
            "-f",
            "/home/j/.axon/compose/docker-compose.yaml",
            "up",
            "-d",
            "axon",
            "--no-deps",
            "--no-build",
            "--force-recreate",
        ]
    );
}

#[test]
fn compose_resolution_rejects_caller_cwd_files() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    fs::write(
        cwd.path().join("docker-compose.yaml"),
        "services: {evil: {}}\n",
    )
    .unwrap();
    fs::write(cwd.path().join(".env"), "EVIL=1\n").unwrap();

    let result = resolve_compose_paths_from_home(&temp.path().join(".axon"), None);

    let err = result.unwrap_err().to_string();
    assert!(err.contains("trusted compose file is missing"));
    assert!(
        err.contains(
            temp.path()
                .join(".axon/compose/docker-compose.yaml")
                .to_string_lossy()
                .as_ref()
        )
    );
}

#[test]
fn explicit_env_file_must_be_absolute() {
    let temp = tempfile::tempdir().unwrap();
    let axon_home = temp.path().join(".axon");
    fs::create_dir_all(&axon_home).unwrap();
    fs::write(axon_home.join(".env"), "TEST_KEY=1\n").unwrap();

    assert_eq!(
        resolve_axon_env_file(&axon_home, Some(Path::new("relative.env"))),
        None
    );
    assert_eq!(
        resolve_axon_env_file(&axon_home, None),
        Some(axon_home.join(".env"))
    );
}
