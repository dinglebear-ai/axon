use super::*;

#[cfg(unix)]
#[tokio::test]
async fn starts_initializes_requests_and_stops_fake_server() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(
        &binary,
        r#"#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    message=json.loads(line)
    if "id" not in message: continue
    method=message["method"]
    result={"userAgent":"fake"} if method=="initialize" else {"models":[{"id":"gpt-test"}]}
    print(json.dumps({"id":message["id"],"result":result}), flush=True)
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let config = ControlConfig {
        enabled: true,
        codex_binary: binary,
        control_home: home,
        request_timeout: Duration::from_secs(2),
        read_concurrency: 2,
        max_restart_backoff: Duration::from_secs(30),
    };
    let transport = ControlTransport::start(&config, RuntimeEpoch(1))
        .await
        .unwrap();
    let result = transport.request("model/list", json!({})).await.unwrap();
    assert_eq!(result["models"][0]["id"], "gpt-test");
    transport.stop().await.unwrap();
}
