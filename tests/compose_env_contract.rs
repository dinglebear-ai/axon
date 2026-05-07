use std::fs;

#[test]
fn services_compose_reads_repo_root_env() {
    let compose = fs::read_to_string("docker-compose.yaml")
        .expect("docker-compose.yaml should be readable at repo root");

    assert!(
        compose.contains("./.env"),
        "docker-compose.yaml must reference ./.env so the repo-root env file is used"
    );
}

#[test]
fn ci_env_file_contains_compose_interpolation_values() {
    let workflow = fs::read_to_string(".github/workflows/ci.yml")
        .expect(".github/workflows/ci.yml should be readable");

    let env_block_start = workflow
        .find("} > .env")
        .expect("CI workflow should create a repo-root .env file");
    let env_block = &workflow[..env_block_start];

    for key in [
        "TEI_HTTP_PORT=52000",
        "TEI_EMBEDDING_MODEL=BAAI/bge-small-en-v1.5",
    ] {
        assert!(
            env_block.contains(key),
            "compose interpolation value {key} must be written to .env"
        );
    }

    assert!(
        workflow.contains("docker compose --env-file .env"),
        "CI should validate compose with the repo-root .env file"
    );
}
