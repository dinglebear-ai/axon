use crate::core::config::{Config, McpTransport};

#[test]
fn config_defaults_to_stdio_transport() {
    let cfg = Config::default();
    assert_eq!(cfg.mcp_transport, McpTransport::Stdio);
    assert_eq!(cfg.mcp_http_host, "127.0.0.1");
    assert_eq!(cfg.mcp_http_port, 8001);
}
