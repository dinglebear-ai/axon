use super::preflight::{NextJsLockState, classify_nextjs_lock_state};
use super::*;
use std::collections::HashSet;

#[test]
fn full_mode_requires_full_infra_set() {
    let cfg = Config::default();
    assert_eq!(
        required_container_services(&cfg),
        &[
            "axon-postgres",
            "axon-redis",
            "axon-rabbitmq",
            "axon-qdrant",
            "axon-tei",
            "axon-chrome",
        ]
    );
}

#[test]
fn lite_mode_requires_only_qdrant_and_tei() {
    let cfg = Config {
        lite_mode: true,
        ..Config::default()
    };
    assert_eq!(
        required_container_services(&cfg),
        &["axon-qdrant", "axon-tei"]
    );
}

#[test]
fn full_mode_supervises_expected_children_without_graph() {
    let cfg = Config::default();
    let specs = supervised_child_specs(&cfg).expect("child specs");
    let names = child_names(&specs);
    assert_eq!(
        names,
        HashSet::from([
            "serve-runtime",
            "mcp-http",
            "shell-server",
            "nextjs",
            "crawl-worker",
            "embed-worker",
            "extract-worker",
            "ingest-worker",
            "refresh-worker",
        ])
    );
}

#[test]
fn full_mode_adds_graph_worker_when_neo4j_is_configured() {
    let cfg = Config {
        neo4j_url: "http://127.0.0.1:7474".to_string(),
        ..Config::default()
    };
    let specs = supervised_child_specs(&cfg).expect("child specs");
    let names = child_names(&specs);
    assert!(names.contains("graph-worker"));
}

#[test]
fn lite_mode_skips_out_of_process_workers() {
    let cfg = Config {
        lite_mode: true,
        ..Config::default()
    };
    let specs = supervised_child_specs(&cfg).expect("child specs");
    let names = child_names(&specs);
    assert_eq!(
        names,
        HashSet::from(["serve-runtime", "mcp-http", "shell-server", "nextjs"])
    );
}

#[test]
fn serve_runtime_child_sets_internal_bridge_role() {
    let cfg = Config::default();
    let specs = supervised_child_specs(&cfg).expect("child specs");
    let serve = specs
        .iter()
        .find(|spec| spec.name == "serve-runtime")
        .expect("serve child");
    assert!(
        serve.env.iter().any(|(key, value)| {
            key == SERVE_CHILD_ROLE_ENV && value == SERVE_CHILD_ROLE_BRIDGE
        })
    );
}

#[test]
fn mcp_http_child_propagates_bind_host_and_port() {
    let cfg = Config {
        mcp_http_host: "0.0.0.0".to_string(),
        mcp_http_port: 8123,
        ..Config::default()
    };
    let specs = supervised_child_specs(&cfg).expect("child specs");
    let mcp = specs
        .iter()
        .find(|spec| spec.name == "mcp-http")
        .expect("mcp-http child");
    assert!(
        mcp.env
            .iter()
            .any(|(key, value)| key == "AXON_MCP_HTTP_HOST" && value == "0.0.0.0")
    );
    assert!(
        mcp.env
            .iter()
            .any(|(key, value)| key == "AXON_MCP_HTTP_PORT" && value == "8123")
    );
}

#[test]
fn restart_backoff_is_bounded() {
    assert_eq!(next_restart_delay(0), 1);
    assert_eq!(next_restart_delay(4), 4);
    assert_eq!(next_restart_delay(90), 30);
}

#[test]
fn extract_pids_parses_ss_output() {
    let line = r#"LISTEN 0 4096 0.0.0.0:8001 0.0.0.0:* users:(("axon",pid=654121,fd=11),("node",pid=42,fd=8))"#;
    assert_eq!(extract_pids_from_ss_line(line), vec![654121, 42]);
}

#[test]
fn parse_matching_processes_finds_next_dev_lines() {
    let raw = "123 next dev --port 49110\n456 node shell-server.mjs\n789 pnpm exec next dev --port 49010\n";
    let matches = parse_matching_processes(raw, &["next dev", "pnpm exec next dev"]);
    assert_eq!(
        matches,
        vec![
            PortOwner {
                pid: 123,
                command: "next dev --port 49110".to_string()
            },
            PortOwner {
                pid: 789,
                command: "pnpm exec next dev --port 49010".to_string()
            }
        ]
    );
}

#[test]
fn port_owner_matching_is_specific_to_expected_processes() {
    let mcp = PortBinding::new("mcp-http", "0.0.0.0", 8001);
    let next = PortBinding::new("nextjs", "127.0.0.1", 49010);

    assert!(port_owner_matches_binding(
        "target/debug/axon mcp --transport http",
        &mcp
    ));
    assert!(!port_owner_matches_binding(
        "python -m http.server 8001",
        &mcp
    ));
    assert!(port_owner_matches_binding("next dev --port 49010", &next));
    assert!(!port_owner_matches_binding("node custom-server.js", &next));
}

#[test]
fn required_bind_ports_follow_config_values() {
    let cfg = Config {
        web_dev_port: 51234,
        shell_server_port: 51235,
        ..Config::default()
    };
    let ports = required_bind_ports(&cfg);
    assert!(ports.contains(&PortBinding::new("nextjs", "127.0.0.1", 51234)));
    assert!(ports.contains(&PortBinding::new("shell-server", "127.0.0.1", 51235)));
}

#[test]
fn child_specs_propagate_configured_web_ports() {
    let cfg = Config {
        web_dev_port: 51234,
        shell_server_port: 51235,
        ..Config::default()
    };
    let specs = supervised_child_specs(&cfg).expect("child specs");

    let next = specs
        .iter()
        .find(|spec| spec.name == "nextjs")
        .expect("nextjs child");
    assert!(next.args.iter().any(|arg| arg == "51234"));

    let shell = specs
        .iter()
        .find(|spec| spec.name == "shell-server")
        .expect("shell child");
    assert!(
        shell
            .env
            .iter()
            .any(|(key, value)| key == "SHELL_SERVER_PORT" && value == "51235")
    );
}

#[test]
fn compose_service_requires_running_and_healthy() {
    let healthy = ComposeServiceStatus {
        service: "axon-qdrant".to_string(),
        name: "axon-qdrant".to_string(),
        state: "running".to_string(),
        health: "healthy".to_string(),
        status: "Up 1m (healthy)".to_string(),
    };
    let unhealthy = ComposeServiceStatus {
        health: "unhealthy".to_string(),
        ..healthy.clone()
    };
    let exited = ComposeServiceStatus {
        state: "exited".to_string(),
        health: String::new(),
        ..healthy.clone()
    };
    assert!(healthy.is_healthy());
    assert!(!unhealthy.is_healthy());
    assert!(!exited.is_healthy());
}

#[test]
fn unstable_restart_limit_trips_after_three_failures() {
    assert!(!reached_unstable_restart_limit(1));
    assert!(!reached_unstable_restart_limit(2));
    assert!(reached_unstable_restart_limit(3));
}

#[test]
fn nextjs_lock_with_active_next_dev_processes_requests_cleanup() {
    let owners = vec![PortOwner {
        pid: 123,
        command: "pnpm exec next dev --port 49010".to_string(),
    }];

    assert_eq!(
        classify_nextjs_lock_state(true, &owners),
        NextJsLockState::TerminateActiveProcesses
    );
}

fn child_names(specs: &[ChildSpec]) -> HashSet<&str> {
    specs.iter().map(|spec| spec.name.as_str()).collect()
}
