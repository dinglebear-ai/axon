#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
INSTALL_SH="$ROOT/install.sh"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

assert_contains() {
  file="$1"
  needle="$2"
  grep -F "$needle" "$file" >/dev/null 2>&1 || {
    printf '--- %s ---\n' "$file" >&2
    cat "$file" >&2 || true
    fail "expected to find: $needle"
  }
}

assert_not_contains() {
  file="$1"
  needle="$2"
  ! grep -F "$needle" "$file" >/dev/null 2>&1 || fail "did not expect to find: $needle"
}

make_exe() {
  path="$1"
  shift
  {
    printf '#!/usr/bin/env sh\n'
    printf '%s\n' "$@"
  } >"$path"
  chmod +x "$path"
}

make_fake_bin() {
  dir="$1"
  mkdir -p "$dir"
  make_exe "$dir/curl" \
    'out=' \
    'while [ "$#" -gt 0 ]; do' \
    '  if [ "$1" = "-o" ]; then shift; out="$1"; fi' \
    '  shift || true' \
    'done' \
    '[ -n "$out" ] || exit 2' \
    'case "$out" in' \
    '  *.sha256) printf "%s  axon\n" "${FAKE_EXPECTED_SHA:-okhash}" >"$out" ;;' \
    '  *) printf "%s\n" "#!/usr/bin/env sh" "printf \"%s\\\\n\" \"\$*\" >> \"\$AXON_TEST_LOG\"" >"$out" ;;' \
    'esac'
  make_exe "$dir/sha256sum" 'printf "%s  %s\n" "${FAKE_ACTUAL_SHA:-okhash}" "$1"'
  make_exe "$dir/install" \
    'last=' \
    'prev=' \
    'for arg do prev="$last"; last="$arg"; done' \
    '[ -n "$prev" ] && [ -n "$last" ] || exit 2' \
    'mkdir -p "$(dirname "$last")"' \
    'cp "$prev" "$last"' \
    'chmod +x "$last"'
  make_exe "$dir/docker" \
    'if [ "${1:-}" = "compose" ] && [ "${2:-}" = "version" ]; then echo "Docker Compose version v2.0.0"; exit 0; fi' \
    'echo "Docker version 26.0.0"'
  make_exe "$dir/nvidia-smi" 'echo "RTX 4070"'
  make_exe "$dir/gemini" 'echo "0.0.0-test"'
}

run_install() {
  work="$1"
  shift
  stdout="$work/stdout"
  stderr="$work/stderr"
  if env "$@" sh "$INSTALL_SH" >"$stdout" 2>"$stderr"; then
    return 0
  else
    status=$?
    return "$status"
  fi
}

test_dry_run_skips_setup_prereqs() {
  work="$(mktemp -d)"
  fake="$work/bin"
  make_fake_bin "$fake"
  rm -f "$fake/docker" "$fake/nvidia-smi" "$fake/gemini"
  if ! run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_DRY_RUN=1; then
    fail "dry run should succeed without setup prereqs"
  fi
  assert_contains "$work/stderr" "Dry run OK"
}

test_checksum_mismatch_fails() {
  work="$(mktemp -d)"
  fake="$work/bin"
  tmp="$work/tmp"
  mkdir -p "$tmp"
  make_fake_bin "$fake"
  if run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_SKIP_SETUP=1 AXON_INSTALL_TMPDIR="$tmp" FAKE_EXPECTED_SHA=expected FAKE_ACTUAL_SHA=actual; then
    fail "checksum mismatch should fail"
  fi
  assert_contains "$work/stderr" "checksum mismatch"
}

test_unsafe_tmpdir_rejected() {
  work="$(mktemp -d)"
  fake="$work/bin"
  make_fake_bin "$fake"
  if run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_SKIP_SETUP=1 AXON_INSTALL_TMPDIR=/; then
    fail "unsafe tmpdir should fail"
  fi
  assert_contains "$work/stderr" "unsafe AXON_INSTALL_TMPDIR"
}

test_skip_setup_does_not_require_runtime_prereqs() {
  work="$(mktemp -d)"
  fake="$work/bin"
  tmp="$work/tmp"
  mkdir -p "$tmp"
  make_fake_bin "$fake"
  rm -f "$fake/docker" "$fake/nvidia-smi" "$fake/gemini"
  if ! run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_PREFIX="$work/prefix" AXON_INSTALL_SKIP_SETUP=1 AXON_INSTALL_TMPDIR="$tmp"; then
    fail "skip setup should not require Docker/Gemini/NVIDIA"
  fi
  [ -x "$work/prefix/bin/axon" ] || fail "axon binary was not installed"
}

test_success_delegates_to_setup_without_logging_token() {
  work="$(mktemp -d)"
  fake="$work/bin"
  tmp="$work/tmp"
  log="$work/axon.log"
  mkdir -p "$tmp"
  make_fake_bin "$fake"
  if ! run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_PREFIX="$work/prefix" AXON_INSTALL_TMPDIR="$tmp" AXON_TEST_LOG="$log" AXON_HTTP_TOKEN="secret-token"; then
    fail "install with setup should succeed"
  fi
  assert_contains "$log" "setup"
  assert_contains "$work/stderr" "Gemini CLI found; axon setup ask-smoke verifies auth and completion"
  assert_not_contains "$work/stderr" "secret-token"
}

test_unsupported_platform_fails() {
  work="$(mktemp -d)"
  fake="$work/bin"
  make_fake_bin "$fake"
  make_exe "$fake/uname" \
    'if [ "${1:-}" = "-s" ]; then echo "Plan9"; else echo "mips"; fi'
  if run_install "$work" PATH="$fake:$PATH" HOME="$work/home" AXON_INSTALL_DRY_RUN=1; then
    fail "unsupported platform should fail"
  fi
  assert_contains "$work/stderr" "unsupported platform"
}

test_dry_run_skips_setup_prereqs
test_checksum_mismatch_fails
test_unsafe_tmpdir_rejected
test_skip_setup_does_not_require_runtime_prereqs
test_success_delegates_to_setup_without_logging_token
test_unsupported_platform_fails

printf 'ok - install.sh behavior tests passed\n'
