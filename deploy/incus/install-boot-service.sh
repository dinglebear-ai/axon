#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
wrapper=/usr/local/libexec/axon-incus-bootstrap
unit=/etc/systemd/system/axon-incus-bootstrap.service

install -d -m 0755 /usr/local/libexec
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
printf '#!/usr/bin/env bash\nexec %q/deploy/incus/bootstrap.sh "$@"\n' "$repo_root" >"$tmp"
install -m 0755 "$tmp" "$wrapper"
install -m 0644 "$repo_root/deploy/incus/axon-incus-bootstrap.service" "$unit"
test -x "$wrapper"
grep -Fqx "ExecStart=$wrapper external-qdrant" "$unit"
systemctl daemon-reload
systemctl enable --now axon-incus-bootstrap.service
systemctl is-enabled --quiet axon-incus-bootstrap.service
