use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn services_compose_reads_canonical_axon_home_env() {
    let compose = fs::read_to_string("docker-compose.yaml")
        .expect("docker-compose.yaml should be readable at repo root");

    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/.env"),
        "docker-compose.yaml must reference ~/.axon/.env so the canonical env file is used"
    );
    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/qdrant"),
        "docker-compose.yaml must keep qdrant data under the canonical ~/.axon appdata root"
    );
    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/tei"),
        "docker-compose.yaml must keep TEI data under the canonical ~/.axon appdata root"
    );
    assert!(
        compose.contains("${AXON_MCP_HTTP_PUBLISH:-127.0.0.1:8001}:8001"),
        "docker-compose.yaml must keep MCP HTTP loopback-only by default"
    );
    assert!(
        compose.contains("$${AXON_MCP_HTTP_TOKEN:-}"),
        "docker-compose.yaml healthcheck must read the token from the container env"
    );
    assert!(
        compose.contains("AXON_HOME: /home/axon/.axon"),
        "docker-compose.yaml must override host AXON_HOME inside the container"
    );
}

#[test]
fn dockerfile_secret_scan_fails_closed() {
    let dockerfile =
        fs::read_to_string("config/Dockerfile").expect("Dockerfile should be readable");

    assert!(
        dockerfile.contains("status=\"$?\"") && dockerfile.contains("\"$status\" -ne 1"),
        "Dockerfile secret scan should treat grep errors as build failures"
    );
}

#[test]
fn ci_env_file_contains_compose_interpolation_values() {
    let workflow = fs::read_to_string(".github/workflows/ci.yml")
        .expect(".github/workflows/ci.yml should be readable");

    let env_block_start = workflow
        .find("} > \"$HOME/.axon/.env\"")
        .expect("CI workflow should create a canonical ~/.axon/.env file");
    let env_block = &workflow[..env_block_start];

    for key in [
        "TEI_HTTP_PORT=52000",
        "TEI_EMBEDDING_MODEL=BAAI/bge-small-en-v1.5",
        "AXON_DATA_DIR=$HOME/.axon",
    ] {
        assert!(
            env_block.contains(key),
            "compose interpolation value {key} must be written to ~/.axon/.env"
        );
    }

    assert!(
        workflow.contains("docker compose --env-file \"$HOME/.axon/.env\""),
        "CI should validate compose with the canonical ~/.axon/.env file"
    );
}

#[test]
fn version_bearing_files_stay_in_sync() {
    let cargo = fs::read_to_string("Cargo.toml").expect("Cargo.toml should be readable");
    let package =
        fs::read_to_string("apps/web/package.json").expect("web package should be readable");
    let changelog = fs::read_to_string("CHANGELOG.md").expect("CHANGELOG should be readable");
    let readme = fs::read_to_string("README.md").expect("README should be readable");

    let version_line = cargo
        .lines()
        .find(|line| line.starts_with("version = "))
        .expect("Cargo.toml should declare package version");
    let version = version_line
        .trim_start_matches("version = ")
        .trim_matches('"');

    assert!(
        package.contains(&format!("\"version\": \"{version}\"")),
        "apps/web/package.json should match Cargo.toml version"
    );
    assert!(
        changelog.contains(&format!("## [{version}]")),
        "CHANGELOG.md should contain a section for the Cargo.toml version"
    );
    assert!(
        readme.contains(&format!("Version: {version}")),
        "README.md version line should match Cargo.toml version"
    );
}

#[test]
fn plugin_setup_uses_canonical_axon_home() {
    let setup = fs::read_to_string("scripts/plugin-setup.sh")
        .expect("scripts/plugin-setup.sh should be readable");
    let readme =
        fs::read_to_string("plugins/README.md").expect("plugins/README.md should be readable");

    assert!(
        setup.contains("AXON_HOME=\"${AXON_HOME:-${HOME}/.axon}\""),
        "plugin setup should default AXON_HOME to ~/.axon"
    );
    assert!(
        setup.contains("ENV_FILE=\"${AXON_HOME}/.env\""),
        "plugin setup should write the canonical ~/.axon/.env file"
    );
    assert!(
        setup.contains("EnvironmentFile=${ENV_FILE}"),
        "plugin systemd unit should read the canonical env file"
    );
    assert!(
        setup.contains("preserved_env") && setup.contains("managed_keys"),
        "plugin setup should preserve unrelated entries when updating the canonical env file"
    );
    assert!(
        setup.contains("value_from_option_or_env"),
        "plugin setup should preserve existing canonical values when plugin options are omitted"
    );
    assert!(
        setup.contains("systemctl --user enable axon-mcp")
            && setup.contains("systemctl --user restart axon-mcp"),
        "plugin setup should restart an active unit after env or unit changes"
    );
    assert!(
        readme.contains("~/.axon/.env"),
        "plugin docs should document the canonical env path"
    );
}

#[test]
fn plugin_setup_smoke_restarts_when_env_changes() {
    let temp_root =
        std::env::temp_dir().join(format!("axon-plugin-setup-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_root);
    let home = temp_root.join("home");
    let fake_bin = temp_root.join("bin");
    let plugin_root = temp_root.join("plugin");
    fs::create_dir_all(&fake_bin).expect("fake bin dir should be created");
    fs::create_dir_all(plugin_root.join("bin")).expect("plugin bin dir should be created");
    fs::create_dir_all(&home).expect("home dir should be created");

    let axon_bin = plugin_root.join("bin/axon");
    fs::write(&axon_bin, "#!/usr/bin/env bash\nexit 0\n").expect("fake axon should be written");
    let mut axon_perms = fs::metadata(&axon_bin)
        .expect("fake axon metadata")
        .permissions();
    axon_perms.set_mode(0o755);
    fs::set_permissions(&axon_bin, axon_perms).expect("fake axon should be executable");

    let systemctl_log = temp_root.join("systemctl.log");
    let fake_systemctl = fake_bin.join("systemctl");
    fs::write(
        &fake_systemctl,
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$*\" = '--user is-active --quiet axon-mcp' ]; then exit 1; fi\nexit 0\n",
            systemctl_log.display()
        ),
    )
    .expect("fake systemctl should be written");
    let mut systemctl_perms = fs::metadata(&fake_systemctl)
        .expect("fake systemctl metadata")
        .permissions();
    systemctl_perms.set_mode(0o755);
    fs::set_permissions(&fake_systemctl, systemctl_perms)
        .expect("fake systemctl should be executable");

    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run_setup = |token: &str| {
        Command::new("bash")
            .arg("scripts/plugin-setup.sh")
            .env("HOME", &home)
            .env("PATH", &path)
            .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
            .env("CLAUDE_PLUGIN_OPTION_API_TOKEN", token)
            .status()
            .expect("plugin setup should run")
    };

    assert!(run_setup("first-token").success());
    assert!(run_setup("second-token").success());

    let log = fs::read_to_string(&systemctl_log).expect("fake systemctl log should exist");
    assert!(
        log.contains("--user daemon-reload") && log.contains("--user enable axon-mcp"),
        "unit changes should reload and enable systemd user service"
    );
    assert!(
        log.matches("--user restart axon-mcp").count() >= 2,
        "initial setup and env changes should restart the service"
    );

    let env_file = fs::read_to_string(home.join(".axon/.env")).expect("env should be written");
    assert!(
        env_file.contains("AXON_MCP_HTTP_TOKEN=second-token"),
        "rerun should update managed token in canonical env"
    );

    fs::remove_dir_all(&temp_root).ok();
}

#[test]
fn services_up_starts_only_infrastructure_services() {
    let justfile = fs::read_to_string("Justfile").expect("Justfile should be readable");

    assert!(
        justfile.contains("up -d axon-qdrant axon-tei axon-chrome"),
        "just services-up should keep its infrastructure-only contract"
    );
    assert!(
        justfile.contains("stop axon-qdrant axon-tei axon-chrome")
            && justfile.contains("rm -f axon-qdrant axon-tei axon-chrome"),
        "just services-down should stop only infrastructure services"
    );
    assert!(
        !justfile.contains("-f docker-compose.yaml down"),
        "just services-down must not tear down the whole compose project"
    );
}

#[test]
fn mcporter_prefers_canonical_env_with_repo_fallback() {
    let config = fs::read_to_string("config/mcporter.json")
        .expect("config/mcporter.json should be readable");

    assert!(
        config.contains("scripts/mcporter-axon"),
        "mcporter config should delegate shell setup to a wrapper script"
    );
    assert!(
        !config.contains("\"AXON_HOME\": \"${HOME}/.axon\""),
        "mcporter static env must not preserve a literal ${{HOME}} value"
    );

    let wrapper =
        fs::read_to_string("scripts/mcporter-axon").expect("mcporter wrapper should be readable");
    assert!(
        wrapper.contains("load_axon_env_file \"$REPO_ROOT\""),
        "mcporter wrapper should prefer the shared canonical env loader"
    );
    assert!(
        wrapper.contains("AXON_HOME=\"${AXON_HOME:-$HOME/.axon}\""),
        "mcporter wrapper should compute AXON_HOME after env loading"
    );
}

#[test]
fn dev_setup_does_not_seed_removed_full_stack_services() {
    let setup = fs::read_to_string("scripts/dev-setup.sh")
        .expect("scripts/dev-setup.sh should be readable");

    for legacy_key in [
        "POSTGRES_PASSWORD",
        "AXON_PG_URL",
        "REDIS_PASSWORD",
        "AXON_REDIS_URL",
        "RABBITMQ_PASS",
        "AXON_AMQP_URL",
        "AXON_TEST_PG_URL",
        "AXON_TEST_REDIS_URL",
        "AXON_TEST_AMQP_URL",
    ] {
        assert!(
            !setup.contains(legacy_key),
            "dev setup should not seed legacy full-stack key {legacy_key} into ~/.axon/.env"
        );
    }
    assert!(
        setup.contains("set_env_if_missing AXON_MCP_HTTP_TOKEN"),
        "dev setup should seed a token for Compose full-stack axon startup"
    );
}

#[test]
fn dev_setup_keeps_axon_home_and_data_dir_aligned() {
    let setup = fs::read_to_string("scripts/dev-setup.sh")
        .expect("scripts/dev-setup.sh should be readable");

    assert!(
        setup.contains("read -r -p \"  AXON_HOME"),
        "dev setup should prompt for AXON_HOME as the relocation knob"
    );
    assert!(
        setup.contains("AXON_DATA_DIR=\"$AXON_HOME\""),
        "dev setup should align AXON_DATA_DIR with AXON_HOME"
    );
    assert!(
        !setup.contains("read -r -p \"  AXON_DATA_DIR"),
        "dev setup should not prompt separately for AXON_DATA_DIR"
    );
    assert!(
        setup.contains("Migrated existing env to"),
        "dev setup should migrate an existing canonical env when AXON_HOME relocates"
    );
    assert!(
        setup.contains("Moved initial env to"),
        "dev setup should move the initial env when an interactive AXON_HOME override relocates it"
    );
}

#[test]
fn shell_scripts_share_canonical_env_resolution() {
    let helper =
        fs::read_to_string("scripts/lib/axon-env.sh").expect("env helper should be readable");

    assert!(
        helper.contains("resolve_axon_env_file") && helper.contains("load_axon_env_file"),
        "shared env helper should expose resolution and loading functions"
    );

    for path in [
        "scripts/axon",
        "scripts/searxng-research",
        "scripts/time-query-gen",
        "scripts/live-test-all-commands.sh",
    ] {
        let script = fs::read_to_string(path).expect("script should be readable");
        assert!(
            script.contains("scripts/lib/axon-env.sh") || script.contains("lib/axon-env.sh"),
            "{path} should use the shared canonical env resolver"
        );
    }
}
