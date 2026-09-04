#!/usr/bin/env bash
set -euo pipefail
# Convenience launcher only. The isolation contract lives in portable Python.
exec python3 "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/run-isolation.py" "$@"
