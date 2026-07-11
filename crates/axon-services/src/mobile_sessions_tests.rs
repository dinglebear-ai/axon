use super::*;

#[test]
fn rejects_path_like_ids() {
    assert!(validate_id("../nope").is_err());
    assert!(validate_id("abc/def").is_err());
    assert!(validate_id("ok-123_ABC").is_ok());
}

#[test]
fn pinned_sessions_sort_first_then_recent() {
    let mut sessions = vec![
        MobileSessionSummary {
            id: "a".into(),
            title: "A".into(),
            first_message_preview: String::new(),
            turn_count: 0,
            injected_op_count: 0,
            created_at: 1,
            updated_at: 10,
            pinned_at: None,
        },
        MobileSessionSummary {
            id: "b".into(),
            title: "B".into(),
            first_message_preview: String::new(),
            turn_count: 0,
            injected_op_count: 0,
            created_at: 1,
            updated_at: 5,
            pinned_at: Some(20),
        },
    ];
    sort_summaries(&mut sessions);
    assert_eq!(sessions[0].id, "b");
}

#[test]
fn sessions_are_owner_scoped() {
    let mut store = BTreeMap::new();

    upsert_into_store(&mut store, "owner-a", "shared", test_session("shared", 100))
        .expect("owner a insert");

    upsert_into_store(&mut store, "owner-b", "shared", test_session("shared", 200))
        .expect("owner b insert");

    assert_eq!(
        store
            .get(&store_key("owner-b", "shared"))
            .unwrap()
            .updated_at,
        200
    );
    assert!(store_key_owner_matches(
        &store_key("owner-a", "shared"),
        "owner-a"
    ));
    assert!(!store_key_owner_matches(
        &store_key("owner-a", "shared"),
        "owner-c"
    ));
}

#[test]
fn rejects_stale_updates() {
    let mut store = BTreeMap::new();

    upsert_into_store(&mut store, "owner", "shared", test_session("shared", 200))
        .expect("initial insert");

    let stale = upsert_into_store(&mut store, "owner", "shared", test_session("shared", 150));
    assert!(matches!(stale, Err(MobileSessionError::StaleUpdate)));
    assert_eq!(
        store.get(&store_key("owner", "shared")).unwrap().updated_at,
        200
    );
}

#[test]
fn rejects_inconsistent_denormalized_counts() {
    let mut store = BTreeMap::new();
    let mut session = test_session("shared", 200);
    session.turn_count = 1;

    let result = upsert_into_store(&mut store, "owner", "shared", session);

    assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
}

#[test]
fn migrates_legacy_unscoped_sessions_to_current_owner() {
    let mut store = BTreeMap::new();
    store.insert("shared".to_string(), test_session("shared", 200));

    assert!(migrate_legacy_entries(&mut store, "owner"));

    assert!(!store.contains_key("shared"));
    assert!(store.contains_key(&store_key("owner", "shared")));
}

#[test]
fn legacy_migration_does_not_overwrite_owner_session() {
    let mut store = BTreeMap::new();
    store.insert("shared".to_string(), test_session("shared", 100));
    store.insert(store_key("owner", "shared"), test_session("shared", 200));

    assert!(migrate_legacy_entries(&mut store, "owner"));

    assert_eq!(
        store.get(&store_key("owner", "shared")).unwrap().updated_at,
        200
    );
}

#[test]
fn mobile_session_status_serializes_snake_case() {
    let statuses = [
        (MobileSessionStatus::Active, "\"active\""),
        (MobileSessionStatus::Archived, "\"archived\""),
        (MobileSessionStatus::Deleted, "\"deleted\""),
        (MobileSessionStatus::SyncConflict, "\"sync_conflict\""),
    ];
    for (status, expected) in statuses {
        let json = serde_json::to_string(&status).expect("serialize status");
        assert_eq!(json, expected);
        let round_tripped: MobileSessionStatus =
            serde_json::from_str(&json).expect("deserialize status");
        assert_eq!(round_tripped, status);
    }
}

#[test]
fn mobile_session_defaults_new_fields_when_omitted() {
    // Older clients/persisted rows without the new fields must still parse.
    let json = serde_json::json!({
        "id": "shared",
        "title": "Test",
        "first_message_preview": "",
        "turn_count": 0,
        "injected_op_count": 0,
        "created_at": 1,
        "updated_at": 1,
    });
    let session: MobileSession = serde_json::from_value(json).expect("deserialize session");
    assert_eq!(session.status, MobileSessionStatus::Active);
    assert!(session.source_refs.is_empty());
    assert_eq!(session.draft, None);
    assert_eq!(session.sync_version, 0);
}

#[test]
fn upsert_assigns_initial_sync_version_and_increments_on_update() {
    let mut store = BTreeMap::new();

    upsert_into_store(&mut store, "owner", "shared", test_session("shared", 100))
        .expect("initial insert");
    assert_eq!(
        store
            .get(&store_key("owner", "shared"))
            .unwrap()
            .sync_version,
        1
    );

    let mut next = test_session("shared", 200);
    next.sync_version = 1;
    upsert_into_store(&mut store, "owner", "shared", next).expect("second upsert");
    assert_eq!(
        store
            .get(&store_key("owner", "shared"))
            .unwrap()
            .sync_version,
        2
    );
}

#[test]
fn rejects_stale_sync_version_even_with_newer_updated_at() {
    let mut store = BTreeMap::new();

    upsert_into_store(&mut store, "owner", "shared", test_session("shared", 100))
        .expect("initial insert");
    // Stored sync_version is now 1. Submit a newer `updated_at` but a stale
    // (default 0) `sync_version` -- this must still be rejected as a
    // conflict, since the client didn't base its edit on the current state.
    let stale_version = test_session("shared", 999);

    let result = upsert_into_store(&mut store, "owner", "shared", stale_version);

    assert!(matches!(result, Err(MobileSessionError::StaleUpdate)));
    assert_eq!(
        store.get(&store_key("owner", "shared")).unwrap().updated_at,
        100
    );
}

#[test]
fn rejects_oversized_source_refs_collection() {
    let mut store = BTreeMap::new();
    let mut session = test_session("shared", 200);
    session.source_refs = (0..257).map(|n| n.to_string()).collect();

    let result = upsert_into_store(&mut store, "owner", "shared", session);

    assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
}

#[test]
fn rejects_oversized_source_ref_entry() {
    let mut store = BTreeMap::new();
    let mut session = test_session("shared", 200);
    session.source_refs = vec!["x".repeat(513)];

    let result = upsert_into_store(&mut store, "owner", "shared", session);

    assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
}

#[test]
fn rejects_oversized_draft() {
    let mut store = BTreeMap::new();
    let mut session = test_session("shared", 200);
    session.draft = Some("x".repeat(8193));

    let result = upsert_into_store(&mut store, "owner", "shared", session);

    assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
}

#[test]
fn rejects_negative_sync_version() {
    let mut store = BTreeMap::new();
    let mut session = test_session("shared", 200);
    session.sync_version = -1;

    let result = upsert_into_store(&mut store, "owner", "shared", session);

    assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
}

fn test_session(id: &str, updated_at: i64) -> MobileSession {
    MobileSession {
        id: id.to_string(),
        title: "Test".into(),
        first_message_preview: String::new(),
        turn_count: 0,
        injected_op_count: 0,
        created_at: 1,
        updated_at,
        pinned_at: None,
        items: Vec::new(),
        status: MobileSessionStatus::default(),
        source_refs: Vec::new(),
        draft: None,
        sync_version: 0,
    }
}
