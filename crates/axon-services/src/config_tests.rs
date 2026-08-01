use super::*;
use std::io::ErrorKind;
use tempfile::TempDir;

#[test]
fn env_round_trip_set_get_unset() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    set_env_entry(&path, "QDRANT_URL", "http://localhost:53333").unwrap();
    set_env_entry(&path, "TAVILY_API_KEY", "secret-value").unwrap();
    let entries = read_env_entries(&path).unwrap();
    assert_eq!(
        entries.get("QDRANT_URL").map(String::as_str),
        Some("http://localhost:53333")
    );
    assert_eq!(
        entries.get("TAVILY_API_KEY").map(String::as_str),
        Some("secret-value")
    );

    let removed = unset_env_entry(&path, "TAVILY_API_KEY").unwrap();
    assert!(removed);
    assert!(
        !read_env_entries(&path)
            .unwrap()
            .contains_key("TAVILY_API_KEY")
    );
    assert!(!unset_env_entry(&path, "TAVILY_API_KEY").unwrap());
}

#[test]
fn env_rejects_invalid_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    let err = set_env_entry(&path, "1bad-key", "x").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
}

#[test]
fn env_quotes_values_with_spaces() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    set_env_entry(&path, "QUOTED", "hello world").unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("QUOTED='hello world'"));
}

#[test]
fn raw_env_text_validates_before_write() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");

    write_env_text(
        &path,
        "QDRANT_URL=http://localhost:53333\nTAVILY_API_KEY='secret value'\n",
    )
    .unwrap();
    assert_eq!(
        read_env_entries(&path)
            .unwrap()
            .get("TAVILY_API_KEY")
            .map(String::as_str),
        Some("secret value")
    );

    let err = write_env_text(&path, "BROKEN='unterminated\n").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn toml_set_get_unset_nested() {
    let mut doc = toml_edit::DocumentMut::new();
    set_toml_entry(&mut doc, "ask.cache.enabled", "true").unwrap();
    set_toml_entry(&mut doc, "ask.cache.ttl-secs", "120").unwrap();
    set_toml_entry(&mut doc, "search.collection", "cortex").unwrap();

    assert_eq!(
        get_toml_entry(&doc, "ask.cache.enabled").as_deref(),
        Some("true")
    );
    assert_eq!(
        get_toml_entry(&doc, "ask.cache.ttl-secs").as_deref(),
        Some("120")
    );
    assert_eq!(
        get_toml_entry(&doc, "search.collection").as_deref(),
        Some("cortex")
    );

    let flat = flatten_toml(&doc);
    assert_eq!(
        flat.get("ask.cache.enabled").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        flat.get("search.collection").map(String::as_str),
        Some("cortex")
    );

    assert!(unset_toml_entry(&mut doc, "ask.cache.ttl-secs").unwrap());
    assert!(get_toml_entry(&doc, "ask.cache.ttl-secs").is_none());
    assert!(!unset_toml_entry(&mut doc, "ask.cache.ttl-secs").unwrap());

    assert!(unset_toml_entry(&mut doc, "search.collection").unwrap());
    assert!(
        !doc.as_table().contains_key("search"),
        "unsetting the last key must remove its empty parent section"
    );
}

#[test]
fn toml_scalar_parsing_picks_correct_types() {
    let mut doc = toml_edit::DocumentMut::new();
    set_toml_entry(&mut doc, "x.bool", "true").unwrap();
    set_toml_entry(&mut doc, "x.int", "42").unwrap();
    set_toml_entry(&mut doc, "x.float", "3.14").unwrap();
    set_toml_entry(&mut doc, "x.str", "hello").unwrap();
    let raw = doc.to_string();
    assert!(raw.contains("bool = true"));
    assert!(raw.contains("int = 42"));
    assert!(raw.contains("float = 3.14"));
    assert!(raw.contains("str = \"hello\""));
}

#[test]
fn secret_detection_matches_registry_and_heuristic() {
    assert!(is_secret_env_key("TAVILY_API_KEY"));
    assert!(is_secret_env_key("GITHUB_TOKEN"));
    assert!(is_secret_env_key("REDDIT_CLIENT_SECRET"));
    assert!(is_secret_env_key("CUSTOM_PASSWORD"));
    assert!(is_secret_env_key("ANYTHING_TOKEN"));
    assert!(!is_secret_env_key("QDRANT_URL"));
    assert!(!is_secret_env_key("TEI_URL"));
}

#[test]
fn redact_returns_empty_for_empty_value() {
    assert_eq!(redact(""), "");
    assert_eq!(redact("hello"), "***");
}

#[test]
fn rewrite_preview_reports_removed_keys_without_writing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(
        &path,
        "AXON_MCP_HTTP_TOKEN=secret\nQDRANT_URL=http://qdrant\n",
    )
    .unwrap();

    let preview = config_rewrite_preview_for_paths(Some(path.clone()), None).unwrap();

    assert!(preview.dry_run);
    assert_eq!(preview.write_count, 0);
    assert_eq!(preview.stale_keys.len(), 1);
    assert_eq!(preview.stale_keys[0].removed_key, "AXON_MCP_HTTP_TOKEN");
    assert_eq!(preview.stale_keys[0].replacement, "AXON_HTTP_TOKEN");
    assert_eq!(preview.stale_keys[0].value_preview, "<redacted>");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "AXON_MCP_HTTP_TOKEN=secret\nQDRANT_URL=http://qdrant\n"
    );
}

#[test]
fn rewrite_apply_moves_env_and_toml_keys() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    std::fs::write(
        &env_path,
        "AXON_MCP_HTTP_TOKEN=secret\nAXON_COLLECTION=rewritten_collection\nQDRANT_URL=http://qdrant\n",
    )
    .unwrap();

    let result =
        config_rewrite_apply_for_paths(Some(env_path.clone()), Some(toml_path.clone())).unwrap();

    assert!(!result.dry_run);
    assert_eq!(result.write_count, 2);
    let env = read_env_entries(&env_path).unwrap();
    assert_eq!(
        env.get("AXON_HTTP_TOKEN").map(String::as_str),
        Some("secret")
    );
    assert!(!env.contains_key("AXON_MCP_HTTP_TOKEN"));
    assert!(!env.contains_key("AXON_COLLECTION"));
    let toml = std::fs::read_to_string(toml_path).unwrap();
    assert!(toml.contains("default-collection = \"rewritten_collection\""));
    axon_core::config::parse::validate_toml_config_text(&toml).unwrap();
}

#[test]
fn rewrite_apply_refuses_conflicting_destination() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "AXON_MCP_HTTP_TOKEN=old\nAXON_HTTP_TOKEN=new\n").unwrap();
    let before = std::fs::read_to_string(&env_path).unwrap();

    let error = config_rewrite_apply_for_paths(Some(env_path.clone()), None).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read_to_string(env_path).unwrap(), before);
}

#[test]
fn rewrite_apply_is_idempotent_when_toml_already_has_the_same_value() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    std::fs::write(&env_path, "AXON_COLLECTION=rewritten_collection\n").unwrap();
    std::fs::write(
        &toml_path,
        "[server]\ndefault-collection = \"rewritten_collection\"\n",
    )
    .unwrap();

    let result =
        config_rewrite_apply_for_paths(Some(env_path.clone()), Some(toml_path.clone())).unwrap();

    assert_eq!(result.write_count, 1);
    assert!(
        !read_env_entries(&env_path)
            .unwrap()
            .contains_key("AXON_COLLECTION")
    );
    assert_eq!(
        get_toml_entry(
            &read_toml_document(&toml_path).unwrap(),
            "server.default-collection"
        )
        .as_deref(),
        Some("rewritten_collection")
    );
}

#[test]
fn rewrite_commit_rolls_back_both_files_after_intermediate_failure() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    let original_env = "AXON_COLLECTION=old\n";
    let original_toml = "[server]\ndefault-collection = \"old\"\n";
    std::fs::write(&env_path, original_env).unwrap();
    std::fs::write(&toml_path, original_toml).unwrap();

    let env_entries = BTreeMap::new();
    let mut document = read_toml_document(&toml_path).unwrap();
    set_toml_entry(&mut document, "server.default-collection", "new").unwrap();
    let error = commit_config_rewrite(
        &env_path,
        &env_entries,
        Some((&toml_path, &document)),
        || Err(io::Error::other("injected failure after TOML write")),
    )
    .unwrap_err();

    assert!(error.to_string().contains("injected failure"));
    assert_eq!(std::fs::read_to_string(&env_path).unwrap(), original_env);
    assert_eq!(std::fs::read_to_string(&toml_path).unwrap(), original_toml);
    assert!(!rewrite_journal_path(&env_path).exists());
}

#[test]
fn rewrite_recovery_does_not_touch_toml_for_an_env_only_transaction() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    let original_env = "AXON_MCP_HTTP_TOKEN=old\n";
    let current_toml = "[server]\ndefault-collection = \"keep\"\n";
    std::fs::write(&env_path, "AXON_HTTP_TOKEN=new\n").unwrap();
    std::fs::write(&toml_path, current_toml).unwrap();
    let journal = ConfigRewriteJournal {
        env_original: Some(original_env.to_string()),
        toml: None,
    };
    std::fs::write(
        rewrite_journal_path(&env_path),
        serde_json::to_string(&journal).unwrap(),
    )
    .unwrap();

    recover_config_rewrite(&env_path).unwrap();

    assert_eq!(std::fs::read_to_string(&env_path).unwrap(), original_env);
    assert_eq!(std::fs::read_to_string(&toml_path).unwrap(), current_toml);
    assert!(!rewrite_journal_path(&env_path).exists());
}

#[test]
fn rewrite_recovery_removes_toml_created_by_an_interrupted_transaction() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    let original_env = "AXON_COLLECTION=old\n";
    std::fs::write(&env_path, "QDRANT_URL=http://qdrant\n").unwrap();
    std::fs::write(&toml_path, "[server]\ndefault-collection = \"partial\"\n").unwrap();
    let journal = ConfigRewriteJournal {
        env_original: Some(original_env.to_string()),
        toml: Some(ConfigRewriteFileJournal {
            path: toml_path.clone(),
            original: None,
        }),
    };
    std::fs::write(
        rewrite_journal_path(&env_path),
        serde_json::to_string(&journal).unwrap(),
    )
    .unwrap();

    recover_config_rewrite(&env_path).unwrap();

    assert_eq!(std::fs::read_to_string(&env_path).unwrap(), original_env);
    assert!(!toml_path.exists());
    assert!(!rewrite_journal_path(&env_path).exists());
}
