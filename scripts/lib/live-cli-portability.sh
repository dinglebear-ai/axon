#!/usr/bin/env bash
# Portable command shims required by the live harness on stock macOS.

install_live_cli_portability_shims() {
  local shim_dir="$OUTDIR/portable-bin"
  mkdir -p "$shim_dir"
  if ! command -v timeout >/dev/null 2>&1; then
    cat >"$shim_dir/timeout" <<'PY'
#!/usr/bin/python3
import os, signal, subprocess, sys

args = sys.argv[1:]
if args and args[0].startswith("--kill-after="):
    args.pop(0)
seconds = float(args.pop(0).removesuffix("s"))
process = subprocess.Popen(args, start_new_session=True)
try:
    raise SystemExit(process.wait(timeout=seconds))
except subprocess.TimeoutExpired:
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()
    raise SystemExit(124)
PY
    chmod 0755 "$shim_dir/timeout"
  fi
  if ! command -v setsid >/dev/null 2>&1; then
    cat >"$shim_dir/setsid" <<'SH'
#!/usr/bin/env sh
exec perl -MPOSIX -e 'POSIX::setsid() or die $!; exec @ARGV' "$@"
SH
    chmod 0755 "$shim_dir/setsid"
  fi
  PATH="$shim_dir:$PATH"
  export PATH
}
