use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::process::Command;

#[test]
fn services_compose_reads_canonical_axon_home_env() {
    let compose = fs::read_to_string("docker-compose.prod.yaml")
        .expect("docker-compose.prod.yaml should be readable at repo root");

    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/.env"),
        "docker-compose.prod.yaml must reference ~/.axon/.env so the canonical env file is used"
    );
    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/qdrant"),
        "docker-compose.prod.yaml must keep qdrant data under the canonical ~/.axon appdata root"
    );
    assert!(
        compose.contains("${AXON_HOME:-${HOME}/.axon}/tei"),
        "docker-compose.prod.yaml must keep TEI data under the canonical ~/.axon appdata root"
    );
    assert!(
        compose.contains("${AXON_MCP_HTTP_PUBLISH:-8001}:8001"),
        "docker-compose.prod.yaml must use short port syntax without a host/interface prefix"
    );
    assert!(
        compose.contains("TEI_SERVER_MAX_CLIENT_BATCH_SIZE"),
        "compose TEI server batch size must not use TEI_MAX_CLIENT_BATCH_SIZE"
    );
    assert!(
        !compose.contains("${TEI_MAX_CLIENT_BATCH_SIZE:-"),
        "TEI_MAX_CLIENT_BATCH_SIZE is client tuning and must not drive TEI server args"
    );
    assert!(
        compose.contains("http://127.0.0.1:8001/healthz"),
        "docker-compose.prod.yaml healthcheck must work in bearer and OAuth-only auth modes"
    );
    assert!(
        compose.contains("AXON_HOME: /home/axon/.axon"),
        "docker-compose.prod.yaml must override host AXON_HOME inside the container"
    );
    assert!(
        compose.contains("AXON_ENV_FILE: \"\"") && compose.contains("AXON_CONFIG_PATH: \"\""),
        "docker-compose.prod.yaml must clear host-only bootstrap overrides in the container"
    );
}

#[test]
fn dev_compose_mounts_local_debug_binary_runtime() {
    let compose = fs::read_to_string("docker-compose.yaml")
        .expect("docker-compose.yaml should be readable at repo root");

    assert!(
        compose.contains("target: dev-runtime"),
        "docker-compose.yaml should build the dev runtime target, not the full production image"
    );
    assert!(
        compose.contains("${AXON_DEV_TARGET_DIR:-./target/debug}"),
        "docker-compose.yaml should bind-mount the local debug target directory"
    );
    assert!(
        compose.contains("target: /home/axon/.axon/dev"),
        "docker-compose.yaml should mount the debug target where the dev entrypoint expects it"
    );
    assert!(
        compose.contains("/home/axon/.axon/dev/axon"),
        "docker-compose.yaml should run the bind-mounted axon binary"
    );
    assert!(
        !compose.contains("docker-compose.dev.yaml"),
        "docker-compose.yaml should be the dev stack directly, not an overlay requiring docker-compose.dev.yaml"
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
    let readme = fs::read_to_string("plugins/axon/README.md")
        .expect("plugins/axon/README.md should be readable");

    assert!(
        setup.contains("AXON_HOME=\"${AXON_HOME:-${HOME}/.axon}\""),
        "plugin setup should default AXON_HOME to ~/.axon"
    );
    assert!(
        setup.contains("mkdir -p \"${AXON_HOME}\""),
        "plugin setup should ensure the canonical ~/.axon home exists"
    );
    assert!(
        setup.contains("axon setup plugin-hook"),
        "plugin setup should delegate to the binary-owned hook-safe setup path"
    );
    assert!(
        setup.contains("warn_stale_systemd_unit"),
        "plugin setup should warn about stale systemd units without managing systemd"
    );
    assert!(
        !setup.contains("systemctl --user"),
        "plugin setup must not create or manage systemd units"
    );
    assert!(
        setup.contains("export_if_set AXON_MCP_HTTP_TOKEN CLAUDE_PLUGIN_OPTION_API_TOKEN"),
        "plugin setup should pass plugin options through to the shared setup command"
    );
    assert!(
        readme.contains("~/.axon/.env"),
        "plugin docs should document the canonical env path"
    );
}

#[cfg(unix)]
#[test]
fn plugin_setup_smoke_delegates_to_shared_setup() {
    let temp_root =
        std::env::temp_dir().join(format!("axon-plugin-setup-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_root);
    let home = temp_root.join("home");
    let fake_bin = temp_root.join("bin");
    let plugin_root = temp_root.join("plugin");
    fs::create_dir_all(&fake_bin).expect("fake bin dir should be created");
    fs::create_dir_all(plugin_root.join("bin")).expect("plugin bin dir should be created");
    fs::create_dir_all(&home).expect("home dir should be created");

    let axon_log = temp_root.join("axon.log");
    let axon_bin = fake_bin.join("axon");
    fs::write(
        &axon_bin,
        format!(
            "#!/usr/bin/env bash\nprintf '%s|%s|%s\\n' \"$*\" \"${{AXON_HOME:-}}\" \"${{AXON_MCP_HTTP_TOKEN:-}}\" >> '{}'\nexit 0\n",
            axon_log.display()
        ),
    )
    .expect("fake axon should be written");
    let mut axon_perms = fs::metadata(&axon_bin)
        .expect("fake axon metadata")
        .permissions();
    axon_perms.set_mode(0o755);
    fs::set_permissions(&axon_bin, axon_perms).expect("fake axon should be executable");

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
            // Clear AXON_HOME so the script derives it from HOME, not the
            // outer shell environment (which may have AXON_HOME set).
            .env_remove("AXON_HOME")
            .status()
            .expect("plugin setup should run")
    };

    assert!(run_setup("first-token").success());
    assert!(run_setup("second-token").success());

    let log = fs::read_to_string(&axon_log).expect("fake axon log should exist");
    assert!(
        log.lines()
            .all(|line| line.starts_with("setup plugin-hook|")),
        "plugin setup should call the binary-owned hook setup command"
    );
    assert!(
        log.contains("first-token") && log.contains("second-token"),
        "plugin setup should export the current token for shared setup"
    );
    assert!(
        home.join(".axon").is_dir(),
        "plugin setup should create the canonical Axon home before delegation"
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
        !justfile.contains("-f docker-compose.prod.yaml down"),
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
        concat!("AXON_", "PG_URL"),
        "REDIS_PASSWORD",
        concat!("AXON_", "REDIS_URL"),
        "RABBITMQ_PASS",
        concat!("AXON_", "AMQP_URL"),
        concat!("AXON_", "TEST_PG_URL"),
        concat!("AXON_", "TEST_REDIS_URL"),
        concat!("AXON_", "TEST_AMQP_URL"),
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

fn env_example_keys() -> BTreeSet<String> {
    include_str!("../.env.example")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=').map(|(key, _)| key.trim().to_string())
        })
        .collect()
}

#[test]
fn env_example_only_contains_production_runtime_keys() {
    let allowed: BTreeSet<&str> = [
        // Bootstrap + canonical data root
        "AXON_HOME",
        "AXON_DATA_DIR",
        "AXON_IMAGE",
        "AXON_COLLECTION",
        // MCP HTTP transport + auth
        "AXON_MCP_HTTP_PUBLISH",
        "AXON_MCP_HTTP_HOST",
        "AXON_MCP_HTTP_PORT",
        "AXON_MCP_HTTP_TOKEN",
        "AXON_MCP_AUTH_MODE",
        "AXON_MCP_PUBLIC_URL",
        "AXON_MCP_GOOGLE_CLIENT_ID",
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
        "AXON_MCP_AUTH_ADMIN_EMAIL",
        "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        "AXON_MCP_ALLOWED_ORIGINS",
        // Vector stack
        "QDRANT_URL",
        "TEI_URL",
        "TEI_HTTP_PORT",
        "TEI_EMBEDDING_MODEL",
        "TEI_SERVER_MAX_CLIENT_BATCH_SIZE",
        "NVIDIA_VISIBLE_DEVICES",
        "CUDA_VISIBLE_DEVICES",
        // Chrome + scrape stack
        "AXON_CHROME_REMOTE_URL",
        // HTTP behavior — UA overrides
        "AXON_USER_AGENT",
        "AXON_CHROME_USER_AGENT",
        // LLM (Gemini headless)
        "GEMINI_HOME",
        "GEMINI_API_KEY",
        "AXON_HEADLESS_GEMINI_CMD",
        "AXON_HEADLESS_GEMINI_HOME",
        "AXON_HEADLESS_GEMINI_MODEL",
        "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
        "AXON_CHAT_HEADLESS_GEMINI_MODEL",
        "AXON_LLM_BACKEND",
        "AXON_OPENAI_BASE_URL",
        "AXON_OPENAI_MODEL",
        "AXON_SYNTHESIS_OPENAI_MODEL",
        "AXON_CHAT_OPENAI_MODEL",
        "AXON_OPENAI_API_KEY",
        // Logging
        "AXON_LOG_PATH",
        // Ingest + search creds
        "HF_TOKEN",
        "TAVILY_API_KEY",
        "AXON_SEARXNG_URL",
        "GITHUB_TOKEN",
        "GITLAB_TOKEN",
        "GITEA_TOKEN",
        "REDDIT_CLIENT_ID",
        "REDDIT_CLIENT_SECRET",
    ]
    .into_iter()
    .collect();

    let actual = env_example_keys();
    let missing: Vec<_> = allowed
        .iter()
        .filter(|key| !actual.contains(**key))
        .copied()
        .collect();
    let unexpected: Vec<_> = actual
        .iter()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "required production env keys missing from .env.example: {missing:?}"
    );
    assert!(
        unexpected.is_empty(),
        "unexpected production env keys in .env.example: {unexpected:?}"
    );
}
