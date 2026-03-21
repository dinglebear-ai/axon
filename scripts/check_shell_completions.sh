#!/usr/bin/env bash
set -euo pipefail

bin="${1:-./target/debug/axon}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

"$bin" completions bash >"$tmp_dir/axon.bash"
"$bin" completions zsh >"$tmp_dir/_axon"
"$bin" completions fish >"$tmp_dir/axon.fish"

grep -q 'complete -F _axon' "$tmp_dir/axon.bash"
grep -q -- '--render-mode' "$tmp_dir/axon.bash"
grep -q 'auto-switch' "$tmp_dir/axon.bash"
grep -q '#compdef axon' "$tmp_dir/_axon"
grep -q 'render-mode' "$tmp_dir/_axon"
grep -q "complete -c axon" "$tmp_dir/axon.fish"
grep -q 'render-mode' "$tmp_dir/axon.fish"
