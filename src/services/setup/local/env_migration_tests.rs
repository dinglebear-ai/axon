use super::*;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[allow(unsafe_code)]
#[test]
fn migration_backs_up_prunes_known_stale_and_redacts_detail_values() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let previous_home = std::env::var_os("HOME");
    let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
    std::fs::write(
        &env_path,
        "AXON_BATCH_QUEUE=old\nTAVILY_API_KEY=secret-value\nTEI_MAX_CLIENT_BATCH_SIZE=32\n",
    )
    .unwrap();
    unsafe {
        std::env::remove_var("AXON_ENV_FILE");
        std::env::remove_var("AXON_CONFIG_PATH");
        std::env::set_var("HOME", dir.path());
    }

    let result = migrate_env_file(&env_path).unwrap();
    assert!(result.phase.detail.contains("backup="));
    assert!(result.phase.detail.contains("deleted=1"));
    assert!(result.phase.detail.contains("moved_toml=1"));
    assert!(!result.phase.detail.contains("secret-value"));

    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert!(raw.contains("TAVILY_API_KEY=secret-value"));
    assert!(raw.contains("AXON_MCP_HTTP_TOKEN="));
    assert!(!raw.contains("AXON_BATCH_QUEUE"));
    assert!(!raw.contains("TEI_MAX_CLIENT_BATCH_SIZE"));
    let config_raw = config_store::read_config().unwrap();
    assert!(config_raw.contains("max-client-batch-size = 32"));

    unsafe {
        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(previous_config_path) = previous_config_path {
            std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}

#[allow(unsafe_code)]
#[test]
fn migration_prunes_legacy_runtime_delete_keys() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let previous_home = std::env::var_os("HOME");
    let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
    std::fs::write(
        &env_path,
        [
            "AXON_AMQP_URL=amqp://legacy",
            "AXON_LITE=1",
            "AXON_PG_MCP_URL=postgres://legacy-mcp",
            "AXON_PG_URL=postgres://legacy",
            "AXON_REDIS_URL=redis://legacy",
            "TAVILY_API_KEY=secret",
        ]
        .join("\n"),
    )
    .unwrap();
    unsafe {
        std::env::remove_var("AXON_ENV_FILE");
        std::env::remove_var("AXON_CONFIG_PATH");
        std::env::set_var("HOME", dir.path());
    }

    let result = migrate_env_file(&env_path).unwrap();
    assert!(result.phase.detail.contains("deleted=5"));
    assert!(
        result
            .phase
            .detail
            .contains("preserved_unclassified_retained=0")
    );
    let raw = std::fs::read_to_string(&env_path).unwrap();
    for key in [
        "AXON_AMQP_URL",
        "AXON_LITE",
        "AXON_PG_MCP_URL",
        "AXON_PG_URL",
        "AXON_REDIS_URL",
    ] {
        assert!(!raw.contains(key), "{key} should be pruned");
    }
    assert!(raw.contains("TAVILY_API_KEY=secret"));

    unsafe {
        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(previous_config_path) = previous_config_path {
            std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}

#[allow(unsafe_code)]
#[test]
fn migration_retains_matrix_only_runtime_keys_and_quotes_shell_sensitive_values() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let previous_home = std::env::var_os("HOME");
    let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
    std::fs::write(
            &env_path,
            "GEMINI_API_KEY=gemini-secret\nAXON_MCP_HTTP_HOST=0.0.0.0\nTEI_MAX_CONCURRENT_REQUESTS=512\nUNKNOWN_KEEP='value with spaces'\n",
        )
        .unwrap();
    unsafe {
        std::env::remove_var("AXON_ENV_FILE");
        std::env::remove_var("AXON_CONFIG_PATH");
        std::env::set_var("HOME", dir.path());
    }

    let result = migrate_env_file(&env_path).unwrap();
    assert!(
        result
            .phase
            .detail
            .contains("preserved_unclassified_retained=2")
    );
    assert!(result.phase.detail.contains("retained_env=1"));
    assert!(result.phase.detail.contains("compose_env=1"));

    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert!(raw.contains("GEMINI_API_KEY=gemini-secret"));
    assert!(raw.contains("AXON_MCP_HTTP_HOST=0.0.0.0"));
    assert!(raw.contains("TEI_MAX_CONCURRENT_REQUESTS=512"));
    assert!(raw.contains("UNKNOWN_KEEP='value with spaces'"));

    unsafe {
        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(previous_config_path) = previous_config_path {
            std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}

#[allow(unsafe_code)]
#[test]
fn migration_decodes_dotenv_values_and_writes_loadable_toml_only() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let previous_home = std::env::var_os("HOME");
    let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
    std::fs::write(
            &env_path,
            "export TEI_MAX_CLIENT_BATCH_SIZE='32'\nAXON_ASK_CHUNK_LIMIT=\"8\"\nAXON_ASK_HYBRID_CANDIDATES=88\nAXON_LLM_COMPLETION_CONCURRENCY=3\nTAVILY_API_KEY='secret with space'\n",
        )
        .unwrap();
    unsafe {
        std::env::remove_var("AXON_ENV_FILE");
        std::env::remove_var("AXON_CONFIG_PATH");
        std::env::set_var("HOME", dir.path());
    }

    migrate_env_file(&env_path).unwrap();

    let raw_env = std::fs::read_to_string(&env_path).unwrap();
    assert!(raw_env.contains("AXON_LLM_COMPLETION_CONCURRENCY=3"));
    assert!(raw_env.contains("TAVILY_API_KEY='secret with space'"));
    assert!(!raw_env.contains("TEI_MAX_CLIENT_BATCH_SIZE"));
    assert!(!raw_env.contains("AXON_ASK_CHUNK_LIMIT"));
    assert!(!raw_env.contains("AXON_ASK_HYBRID_CANDIDATES"));

    let config_raw = config_store::read_config().unwrap();
    assert!(config_raw.contains("max-client-batch-size = 32"));
    assert!(config_raw.contains("chunk-limit = 8"));
    assert!(config_raw.contains("ask-hybrid-candidates = 88"));
    assert!(!config_raw.contains("[llm]"));
    assert!(!config_raw.contains("ask-chunk-limit"));
    crate::core::config::parse::validate_toml_config_text(&config_raw).unwrap();

    unsafe {
        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(previous_config_path) = previous_config_path {
            std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}

#[cfg(unix)]
#[test]
fn migration_rejects_symlinked_env_before_backup() {
    use std::os::unix::fs::symlink;

    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.env");
    let env_path = dir.path().join(".env");
    std::fs::write(&target, "TAVILY_API_KEY=secret-value\n").unwrap();
    symlink(&target, &env_path).unwrap();

    let err = migrate_env_file(&env_path).unwrap_err();
    assert!(err.to_string().contains("symlinked env file"));
    assert!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".env.backup."))
    );
}

#[allow(unsafe_code)]
#[test]
fn migration_rejects_shadowed_axon_env_file() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "TAVILY_API_KEY=secret-value\n").unwrap();
    unsafe {
        std::env::set_var("AXON_ENV_FILE", dir.path().join("other.env"));
    }

    let err = migrate_env_file(&env_path).unwrap_err();
    assert!(err.to_string().contains("AXON_ENV_FILE is set"));

    unsafe {
        std::env::remove_var("AXON_ENV_FILE");
    }
}
