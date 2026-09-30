#!/usr/bin/env bash
# Read-only drift check. Do not overwrite and restore a developer's document.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
exec python3 "$repo_root/scripts/generate_mcp_schema_doc.py" --repo-root "$repo_root" --check
