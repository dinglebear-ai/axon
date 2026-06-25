use anyhow::{Result, bail};
use std::path::Path;

/// (relative_path, &[(pattern, error_message_if_missing)])
type FileSpec = (&'static str, &'static [(&'static str, &'static str)]);

const FILE_SPECS: &[FileSpec] = &[
    (
        "crates/axon-cli/src/commands/mcp.rs",
        &[
            (
                "run_unified_server(",
                "ERROR: MCP CLI must support unified HTTP transport in crates/axon-cli/src/commands/mcp.rs",
            ),
            (
                "run_stdio_server(",
                "ERROR: MCP CLI must support stdio transport in crates/axon-cli/src/commands/mcp.rs",
            ),
            // Match the actual McpTransport::Both match arm shape, not a bare "Both"
            // substring. The bare token would be satisfied by a comment, an unrelated
            // enum, or even the doc-comment in this file — defeating the gate.
            (
                "McpTransport::Both =>",
                "ERROR: MCP CLI must support both transports concurrently \
                 (`McpTransport::Both =>` arm) in crates/axon-cli/src/commands/mcp.rs",
            ),
        ],
    ),
    (
        "crates/axon-core/src/config/cli.rs",
        &[(
            "transport: Option<McpTransport>",
            "ERROR: MCP CLI must expose --transport in crates/axon-core/src/config/cli.rs",
        )],
    ),
    (
        "crates/axon-core/src/config/parse/build_config/config_literal.rs",
        &[(
            "resolve_mcp_transport(mcp_transport, mcp_transport_default)",
            "ERROR: MCP transport resolver not wired into config build in crates/axon-core/src/config/parse/build_config/config_literal.rs",
        )],
    ),
    (
        "crates/axon-core/src/config/parse/helpers.rs",
        &[(
            "AXON_MCP_TRANSPORT",
            "ERROR: MCP transport env override missing in crates/axon-core/src/config/parse/helpers.rs",
        )],
    ),
];

pub fn check(root: &Path) -> Result<()> {
    for (rel_path, patterns) in FILE_SPECS {
        let path = root.join(rel_path);
        if !path.is_file() {
            bail!("ERROR: missing {}", rel_path);
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("ERROR: failed to read {}: {}", rel_path, e))?;
        for (pattern, err_msg) in *patterns {
            if !contents.contains(pattern) {
                bail!("{}", err_msg);
            }
        }
    }
    println!("OK: MCP CLI supports stdio, http, and both transport modes.");
    Ok(())
}

#[cfg(test)]
#[path = "mcp_http_tests.rs"]
mod tests;
