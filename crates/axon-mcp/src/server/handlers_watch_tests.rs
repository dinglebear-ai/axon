use super::*;

fn watch_test_server() -> (AxonMcpServer, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sqlite_path = tmp.path().join("jobs.db");
    (
        AxonMcpServer::new(axon_core::config::Config {
            sqlite_path,
            ..axon_core::config::Config::default_minimal()
        }),
        tmp,
    )
}

fn inline_data(response: &AxonToolResponse) -> &serde_json::Value {
    response
        .data
        .get("data")
        .unwrap_or_else(|| panic!("expected auto-inline response data: {response:?}"))
}

fn watch_request(subaction: WatchSubaction) -> WatchMcpRequest {
    WatchMcpRequest {
        subaction: Some(subaction),
        id: None,
        every_seconds: None,
        enabled: None,
        limit: None,
        cursor: None,
        status: None,
        collection: None,
        source: None,
        embed: None,
        scope: None,
        options: None,
        reason: None,
        refresh: None,
        wait: None,
        response_mode: None,
    }
}

async fn create_watch(server: &AxonMcpServer, source: &str) -> AxonToolResponse {
    let mut request = watch_request(WatchSubaction::Create);
    request.source = Some(source.to_string());
    request.every_seconds = Some(3600);
    server.handle_watch(request).await.expect("watch create")
}

#[tokio::test]
async fn mcp_watch_exec_and_history_resolve_source_watches_by_source() {
    let (server, _tmp) = watch_test_server();
    let created_source = "https://example.com/docs/";
    let source = "https://example.com/docs";

    let created = create_watch(&server, created_source).await;
    assert_eq!(created.action, "watch");
    assert_eq!(created.subaction, "create");
    let watch_id = inline_data(&created)["watch_id"]
        .as_str()
        .expect("created watch id")
        .to_string();
    assert_eq!(inline_data(&created)["canonical_uri"], source);
    let store = axon_services::watch::open_source_watch_store(server.cfg.as_ref(), None)
        .await
        .expect("watch store");
    let stored = store
        .request(WatchId::new(&watch_id))
        .await
        .expect("stored request")
        .expect("stored request present");
    assert!(stored.embed, "MCP-created watches embed by default");

    let mut exec_request = watch_request(WatchSubaction::Exec);
    exec_request.source = Some(source.to_string());
    let executed = server
        .handle_watch(exec_request)
        .await
        .expect("watch exec by source");
    assert_eq!(executed.action, "watch");
    assert_eq!(executed.subaction, "exec");
    assert_eq!(inline_data(&executed)["kind"], "source");
    assert!(inline_data(&executed)["id"].is_string());

    let mut status_request = watch_request(WatchSubaction::Status);
    status_request.source = Some(source.to_string());
    let status = server
        .handle_watch(status_request)
        .await
        .expect("watch status by source");
    assert_eq!(status.action, "watch");
    assert_eq!(status.subaction, "status");
    assert_eq!(inline_data(&status)["watch"]["watch_id"], watch_id);

    let mut history_request = watch_request(WatchSubaction::History);
    history_request.source = Some(source.to_string());
    history_request.limit = Some(10);
    let history = server
        .handle_watch(history_request)
        .await
        .expect("watch history by source");
    assert_eq!(history.action, "watch");
    assert_eq!(history.subaction, "history");
    assert_eq!(inline_data(&history)["watch_id"], watch_id);
    let jobs = inline_data(&history)["jobs"]
        .as_array()
        .expect("history jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["kind"], "source");
    assert_eq!(jobs[0]["id"], inline_data(&executed)["id"]);
}

#[test]
fn mcp_watch_rejects_legacy_task_payload_at_parse_boundary() {
    let raw = serde_json::json!({
        "action": "watch",
        "subaction": "exec",
        "task_type": "watch",
        "task_payload": {
            "urls": ["https://example.com/legacy"]
        }
    })
    .as_object()
    .expect("object")
    .clone();

    let err = crate::schema::parse_axon_request(raw).expect_err("legacy task payload must fail");
    assert!(
        err.contains("unknown field")
            && (err.contains("task_type") || err.contains("task_payload")),
        "legacy task fields should be rejected by the DTO boundary, got: {err}"
    );
}

#[test]
fn mcp_watch_forwards_list_and_history_cursors_and_status() {
    let mut list = watch_request(WatchSubaction::List);
    list.enabled = Some(true);
    list.limit = Some(25);
    list.cursor = Some("watch-cursor".to_string());
    let projected = watch_list_request(&list);
    assert_eq!(projected.enabled, Some(true));
    assert_eq!(projected.limit, Some(25));
    assert_eq!(projected.cursor.as_deref(), Some("watch-cursor"));

    let mut history = watch_request(WatchSubaction::History);
    history.limit = Some(10);
    history.cursor = Some("history-cursor".to_string());
    history.status = Some(axon_api::source::LifecycleStatus::Failed);
    let projected = watch_history_request(&history, WatchId::new("watch-1"));
    assert_eq!(projected.limit, Some(10));
    assert_eq!(projected.cursor.as_deref(), Some("history-cursor"));
    assert_eq!(
        projected.status,
        Some(axon_api::source::LifecycleStatus::Failed)
    );
}

#[test]
fn mcp_watch_forwards_exec_reason_refresh_and_wait() {
    let mut request = watch_request(WatchSubaction::Exec);
    request.reason = Some("manual verification".to_string());
    request.refresh = Some(axon_api::source::SourceRefreshPolicy::Force);
    request.wait = Some(true);

    let projected = watch_exec_request(&request);
    assert_eq!(projected.reason.as_deref(), Some("manual verification"));
    assert_eq!(
        projected.refresh,
        Some(axon_api::source::SourceRefreshPolicy::Force)
    );
    assert_eq!(projected.wait, Some(true));
}

#[tokio::test]
async fn mcp_watch_create_uses_authenticated_caller_snapshot_for_local_scope() {
    let (server, _tmp) = watch_test_server();
    let mut request = watch_request(WatchSubaction::Create);
    request.source = Some("/tmp/axon-watch-private".to_string());
    request.scope = Some(axon_api::source::SourceScope::Directory);
    request.every_seconds = Some(3600);
    let caller = axon_api::source::AuthSnapshot::from_caller(
        &axon_api::source::CallerContext {
            caller_id: Some("mcp-reviewer".to_string()),
            transport: axon_api::source::TransportKind::Mcp,
            trusted_local: false,
            scopes: vec!["axon:write".to_string()],
            visibility_ceiling: axon_api::source::Visibility::Internal,
            auth_mode: axon_api::source::AuthMode::Oauth,
            token_id: None,
            display_name: None,
        },
        axon_api::source::Visibility::Internal,
        "test",
    );

    let error = CURRENT_CALLER_AUTH_SNAPSHOT
        .scope(Some(caller), server.handle_watch(request))
        .await
        .expect_err("caller without local scope must not create a local watch");
    assert!(error.message.contains("scope") || error.message.contains("local"));
}
