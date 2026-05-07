use anyhow::{Result, bail};
use std::path::Path;

/// (relative_path, &[(pattern, error_message_if_missing)])
type FileSpec = (&'static str, &'static [(&'static str, &'static str)]);

const FILE_SPECS: &[FileSpec] = &[
    (
        "src/cli/commands/mcp.rs",
        &[
            (
                "run_http_server(",
                "ERROR: MCP CLI must support HTTP transport in src/cli/commands/mcp.rs",
            ),
            (
                "run_stdio_server(",
                "ERROR: MCP CLI must support stdio transport in src/cli/commands/mcp.rs",
            ),
            // Match the actual McpTransport::Both match arm shape, not a bare "Both"
            // substring. The bare token would be satisfied by a comment, an unrelated
            // enum, or even the doc-comment in this file — defeating the gate.
            (
                "McpTransport::Both =>",
                "ERROR: MCP CLI must support both transports concurrently \
                 (`McpTransport::Both =>` arm) in src/cli/commands/mcp.rs",
            ),
        ],
    ),
    (
        "src/core/config/cli.rs",
        &[(
            "transport: Option<McpTransport>",
            "ERROR: MCP CLI must expose --transport in src/core/config/cli.rs",
        )],
    ),
    (
        "src/core/config/parse/build_config.rs",
        &[(
            "resolve_mcp_transport(mcp_transport, mcp_transport_default)",
            "ERROR: MCP transport resolver not wired into config build in src/core/config/parse/build_config.rs",
        )],
    ),
    (
        "src/core/config/parse/helpers.rs",
        &[(
            "AXON_MCP_TRANSPORT",
            "ERROR: MCP transport env override missing in src/core/config/parse/helpers.rs",
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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_all_required(root: &Path) {
        let mcp_rs = root.join("src/cli/commands/mcp.rs");
        fs::create_dir_all(mcp_rs.parent().unwrap()).unwrap();
        fs::write(
            &mcp_rs,
            "fn run_http_server() {}\nfn run_stdio_server() {}\n\
             match t { McpTransport::Both => {} }\n",
        )
        .unwrap();

        let cli_cfg = root.join("src/core/config/cli.rs");
        fs::create_dir_all(cli_cfg.parent().unwrap()).unwrap();
        fs::write(
            &cli_cfg,
            "pub struct C { pub transport: Option<McpTransport> }\n",
        )
        .unwrap();

        let build_cfg = root.join("src/core/config/parse/build_config.rs");
        fs::create_dir_all(build_cfg.parent().unwrap()).unwrap();
        fs::write(
            &build_cfg,
            "let t = resolve_mcp_transport(mcp_transport, mcp_transport_default);\n",
        )
        .unwrap();

        let helpers = root.join("src/core/config/parse/helpers.rs");
        fs::create_dir_all(helpers.parent().unwrap()).unwrap();
        fs::write(&helpers, "// reads AXON_MCP_TRANSPORT env var\n").unwrap();
    }

    #[test]
    fn passes_with_all_patterns_present() {
        let tmp = TempDir::new().unwrap();
        write_all_required(tmp.path());
        check(tmp.path()).expect("expected check to pass");
    }

    #[test]
    fn fails_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        write_all_required(tmp.path());
        fs::remove_file(tmp.path().join("src/cli/commands/mcp.rs")).unwrap();
        let err = check(tmp.path()).expect_err("expected missing file error");
        assert_eq!(err.to_string(), "ERROR: missing src/cli/commands/mcp.rs");
    }

    #[test]
    fn fails_when_pattern_missing() {
        let tmp = TempDir::new().unwrap();
        write_all_required(tmp.path());
        // Overwrite mcp.rs missing the `McpTransport::Both =>` arm. A bare `Both`
        // token (e.g., in a comment) must NOT satisfy the matcher.
        fs::write(
            tmp.path().join("src/cli/commands/mcp.rs"),
            "fn run_http_server() {}\nfn run_stdio_server() {}\n// keyword: Both\n",
        )
        .unwrap();
        let err = check(tmp.path()).expect_err("expected pattern error");
        assert!(
            err.to_string().contains("McpTransport::Both =>"),
            "error should reference the strengthened matcher, got: {err}"
        );
    }

    #[test]
    fn pattern_table_is_canonical() {
        // Lock the table shape to catch accidental edits.
        let paths: Vec<&'static str> = FILE_SPECS.iter().map(|(p, _)| *p).collect();
        assert_eq!(
            paths,
            vec![
                "src/cli/commands/mcp.rs",
                "src/core/config/cli.rs",
                "src/core/config/parse/build_config.rs",
                "src/core/config/parse/helpers.rs",
            ]
        );

        let mcp_patterns: Vec<&'static str> = FILE_SPECS[0].1.iter().map(|(p, _)| *p).collect();
        assert_eq!(
            mcp_patterns,
            vec![
                "run_http_server(",
                "run_stdio_server(",
                "McpTransport::Both =>"
            ]
        );

        assert_eq!(FILE_SPECS[1].1.len(), 1);
        assert_eq!(FILE_SPECS[1].1[0].0, "transport: Option<McpTransport>");

        assert_eq!(FILE_SPECS[2].1.len(), 1);
        assert_eq!(
            FILE_SPECS[2].1[0].0,
            "resolve_mcp_transport(mcp_transport, mcp_transport_default)"
        );

        assert_eq!(FILE_SPECS[3].1.len(), 1);
        assert_eq!(FILE_SPECS[3].1[0].0, "AXON_MCP_TRANSPORT");
    }
}
