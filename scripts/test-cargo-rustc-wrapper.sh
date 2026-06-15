#!/usr/bin/env bash
set -euo pipefail

repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

export HOME="$tmp/home"
export AXON_RUSTC_WRAPPER_LOCAL_BIN="$HOME/.local/bin/axon"
export AXON_RUSTC_WRAPPER_NO_SCCACHE=1
mkdir -p "$HOME/.local/bin" "$tmp/target/debug/deps"

fake_rustc="$tmp/fake-rustc"
cat >"$fake_rustc" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
out=""
crate=""
out_dir=""
extra=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --crate-name)
      crate="$2"
      shift 2
      ;;
    -o)
      out="$2"
      shift 2
      ;;
    --out-dir)
      out_dir="$2"
      shift 2
      ;;
    -C)
      case "${2:-}" in
        extra-filename=*) extra="${2#extra-filename=}" ;;
      esac
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
if [ -z "$out" ] && [ -n "$crate" ] && [ -n "$out_dir" ] && [ -n "$extra" ]; then
  out="$out_dir/$crate$extra"
fi
if [ -n "$out" ]; then
  mkdir -p "$(dirname "$out")"
  printf 'fake axon binary\n' >"$out"
  chmod +x "$out"
fi
SH
chmod +x "$fake_rustc"

out="$tmp/target/debug/deps/axon-123"
"$repo/scripts/cargo-rustc-wrapper" "$fake_rustc" \
  --crate-name axon \
  --crate-type bin \
  src/main.rs \
  -o "$out"

cmp "$out" "$HOME/.local/bin/axon"

rm -f "$HOME/.local/bin/axon"
"$repo/scripts/cargo-rustc-wrapper" "$fake_rustc" \
  --crate-name axon \
  --crate-type bin \
  --test \
  src/main.rs \
  -o "$out"

test ! -e "$HOME/.local/bin/axon"

rm -f "$HOME/.local/bin/axon"
(
  cd "$tmp"
  "$repo/scripts/cargo-rustc-wrapper" "$fake_rustc" \
    --crate-name axon \
    --crate-type bin \
    src/main.rs \
    -o target/release/deps/axon-456
)

cmp "$tmp/target/release/deps/axon-456" "$HOME/.local/bin/axon"

rm -f "$HOME/.local/bin/axon"
"$repo/scripts/cargo-rustc-wrapper" "$fake_rustc" \
  --crate-name axon \
  --crate-type bin \
  src/main.rs \
  --out-dir "$tmp/target/debug/deps" \
  -C extra-filename=-789

cmp "$tmp/target/debug/deps/axon-789" "$HOME/.local/bin/axon"

echo "cargo rustc wrapper install behavior ok"
