#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/lib/live-cli-portability.sh"

port="$(python3 - <<'PY'
import socket
s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()
PY
)"
live_cli_port_is_available "$port"
python3 - "$port" <<'PY' &
import socket, sys, time
s=socket.socket(); s.bind(("127.0.0.1", int(sys.argv[1]))); s.listen(); time.sleep(10)
PY
holder=$!
trap 'kill "$holder" 2>/dev/null || true' EXIT
sleep 1
if live_cli_port_is_available "$port"; then
  echo "occupied port reported available" >&2
  exit 1
fi
kill "$holder"; wait "$holder" 2>/dev/null || true
trap - EXIT
echo 'ok - portable port probe passed'
