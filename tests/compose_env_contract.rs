use std::fs;

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
        compose.contains("AXON_HOME: /home/axon/.axon"),
        "docker-compose.yaml must override host AXON_HOME inside the container"
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
        readme.contains("~/.axon/.env"),
        "plugin docs should document the canonical env path"
    );
}

#[test]
fn services_up_starts_only_infrastructure_services() {
    let justfile = fs::read_to_string("Justfile").expect("Justfile should be readable");

    assert!(
        justfile.contains("up -d axon-qdrant axon-tei axon-chrome"),
        "just services-up should keep its infrastructure-only contract"
    );
}

#[test]
fn mcporter_prefers_canonical_env_with_repo_fallback() {
    let config = fs::read_to_string("config/mcporter.json")
        .expect("config/mcporter.json should be readable");

    assert!(
        config.contains("ENV_FILE=\\\"${AXON_ENV_FILE:-$AXON_HOME/.env}\\\""),
        "mcporter config should prefer the canonical env file"
    );
    assert!(
        config.contains("elif [ -f ./.env ]; then source ./.env; fi"),
        "mcporter config may keep repo .env as an explicit development fallback"
    );
    assert!(
        !config.contains("\"AXON_HOME\": \"${HOME}/.axon\""),
        "mcporter static env must not preserve a literal ${{HOME}} value"
    );
    assert!(
        config.contains("AXON_HOME=\\\"${AXON_HOME:-$HOME/.axon}\\\""),
        "mcporter shell command should compute AXON_HOME after env loading"
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
}
