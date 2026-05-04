#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-.}"
cd "$ROOT"

TARGET="crates/cli/commands/mcp.rs"
CLI_CONFIG="crates/core/config/cli.rs"
BUILD_CONFIG="crates/core/config/parse.rs"

if [ ! -f "$TARGET" ]; then
  echo "ERROR: missing $TARGET"
  exit 1
fi

if [ ! -f "$CLI_CONFIG" ]; then
  echo "ERROR: missing $CLI_CONFIG"
  exit 1
fi

if [ ! -f "$BUILD_CONFIG" ]; then
  echo "ERROR: missing $BUILD_CONFIG"
  exit 1
fi

if ! grep -q 'run_http_server(' "$TARGET"; then
  echo "ERROR: MCP CLI must support HTTP transport in $TARGET"
  exit 1
fi

if ! grep -q 'run_stdio_server(' "$TARGET"; then
  echo "ERROR: MCP CLI must support stdio transport in $TARGET"
  exit 1
fi

if ! grep -q 'Both' "$TARGET"; then
  echo "ERROR: MCP CLI must support both transports concurrently in $TARGET"
  exit 1
fi

if ! grep -q 'transport: Option<McpTransport>' "$CLI_CONFIG"; then
  echo "ERROR: MCP CLI must expose --transport in $CLI_CONFIG"
  exit 1
fi

if ! grep -q 'AXON_MCP_TRANSPORT' "$BUILD_CONFIG"; then
  echo "ERROR: MCP transport env override missing in $BUILD_CONFIG"
  exit 1
fi

echo "OK: MCP CLI supports stdio, http, and both transport modes."
