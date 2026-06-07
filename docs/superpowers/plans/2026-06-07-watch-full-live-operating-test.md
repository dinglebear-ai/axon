# Watch Full Live Operating Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and run a repeatable live operating test that proves Axon's watch feature works end-to-end through create, scheduler execution, diff-gated recrawl, run history, artifacts, crawl dispatch, in-flight guards, and cleanup.

**Architecture:** The test must run Axon in production-style server mode with `ServiceContext::new_with_workers`, not only in-process unit tests. Because production URL validation blocks loopback/private addresses, the watched fixture must be a controlled mutable page reachable through a public/non-private HTTP(S) URL; localhost-only tests remain useful regressions but are not full operating proof. The harness should isolate Axon state in a temporary `AXON_HOME`/`AXON_DATA_DIR`, disable embedding with `--skip-embed` unless TEI/Qdrant coverage is explicitly requested, and inspect both HTTP/CLI surfaces plus SQLite state.

**Tech Stack:** Bash, Python 3 fixture server, Axon Rust CLI/server, SQLite (`sqlite3`), curl, jq, optional Docker/SWAG or other public reverse proxy for the mutable fixture.

---

## Code Review Summary

The full operating test must exercise these actual code paths:

- `src/web/server/handlers/rest/admin.rs`: `POST /v1/watch`, `GET /v1/watch`, `GET /v1/watch/{id}`, `POST /v1/watch/{id}/run`
- `src/jobs/workers/watch_scheduler.rs`: due-watch scheduler spawned only by long-running worker contexts such as `axon serve` / `axon mcp`
- `src/jobs/watch.rs`: shared create validation, SQLite definitions/runs/artifacts accessors, due leasing, manual leasing
- `src/jobs/watch/run_now.rs`: run row creation/finalization, lease heartbeat, leased execution path
- `src/jobs/watch/change_detect.rs`: conditional probe, scrape fallback, filtering, hash fast-path, `compute_diff`, meaningfulness threshold
- `src/jobs/watch/orchestrate.rs`: per-URL watch execution, change artifact writing, clustered crawl dispatch
- `src/jobs/watch/dispatch.rs`: crawl enqueue and in-flight crawl guard
- `src/cli/commands/watch.rs`: CLI list/history/artifacts/run-now surfaces

Unit coverage currently proves pieces of this behavior, including `live_watch_only_recrawls_when_page_changes`, but that test uses a test-only loopback bypass. It does not prove the real server process, production URL validation, scheduler loop, REST create path, CLI inspection path, or worker-drained crawl job behavior together.

## Proper Live Test Shape

The correct full live test is:

1. Start a mutable fixture page at a public/non-private URL controlled by the tester.
2. Start `axon serve` with an isolated data root, workers enabled, scheduler tick shortened, and embedding disabled.
3. Create a watch through the production REST route.
4. Wait for the scheduler to fire the due watch automatically.
5. Confirm first run completes, writes `changed=1`, writes a `url-change` artifact, persists URL state, and enqueues a crawl.
6. Wait for the triggered crawl job to reach a terminal state.
7. Wait for the next scheduled interval without changing the page.
8. Confirm second run completes with `changed=0`, no new artifact, no new crawl job.
9. Mutate the fixture page content and validators.
10. Wait for the next scheduled interval.
11. Confirm third run completes with `changed=1`, one additional `url-change` artifact, and exactly one additional crawl job.
12. Attempt a manual `run-now` while a scheduled run is leased, using a slow fixture endpoint, and confirm the single-flight guard rejects the manual run and creates no duplicate run row.
13. Read the same evidence through CLI (`watch list`, `watch history`, `watch artifacts`) and SQLite, then clean up Axon and fixture processes.

## File Structure

- Create: `scripts/live-test-watch.sh`
  - Orchestrates the live test, starts Axon, calls REST endpoints, waits for scheduler/crawl state, mutates the fixture through supplied commands, and emits an evidence table.
- Create: `scripts/live-test-watch-fixture.py`
  - Tiny mutable fixture server for local development. It is not sufficient by itself for production-mode URL validation unless exposed through a public hostname, but it gives operators a known fixture implementation to put behind SWAG/Cloudflare Tunnel/another public proxy.
- Modify: `docs/operations/operations.md`
  - Add a short "Watch Live Operating Test" section with the exact command, fixture URL requirements, and evidence expected.
- Test: manual live test plus shell validation.
  - `bash -n scripts/live-test-watch.sh`
  - `python3 -m py_compile scripts/live-test-watch-fixture.py`
  - Full live command with `AXON_WATCH_LIVE_URL` and fixture mutation hooks set.

---

### Task 1: Add The Mutable Fixture Server

**Files:**
- Create: `scripts/live-test-watch-fixture.py`

- [ ] **Step 1: Write the fixture server**

Create `scripts/live-test-watch-fixture.py`:

```python
#!/usr/bin/env python3
import argparse
import hashlib
import http.server
import json
import os
import socketserver
import sys
import time
from pathlib import Path


class FixtureHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, directory: str, state_file: Path, **kwargs):
        self.state_file = state_file
        super().__init__(*args, directory=directory, **kwargs)

    def log_message(self, fmt: str, *args) -> None:
        event = {
            "ts": time.time(),
            "client": self.client_address[0],
            "method": self.command,
            "path": self.path,
            "if_none_match": self.headers.get("If-None-Match"),
            "if_modified_since": self.headers.get("If-Modified-Since"),
            "status": getattr(self, "_last_status", None),
            "message": fmt % args,
        }
        with self.state_file.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(event, sort_keys=True) + "\n")

    def send_response(self, code: int, message: str | None = None) -> None:
        self._last_status = code
        super().send_response(code, message)

    def end_headers(self) -> None:
        if self.path in {"/", "/index.html", "/slow.html"}:
            target = "slow.html" if self.path == "/slow.html" else "index.html"
            file_path = Path(self.directory) / target
            if file_path.exists():
                body = file_path.read_bytes()
                self.send_header("ETag", hashlib.sha256(body).hexdigest())
        super().end_headers()

    def do_GET(self) -> None:
        if self.path == "/slow.html":
            time.sleep(float(os.environ.get("AXON_WATCH_FIXTURE_SLOW_SECS", "8")))
        super().do_GET()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--port-file", required=True)
    parser.add_argument("--log-file", required=True)
    args = parser.parse_args()

    directory = Path(args.directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "index.html").write_text(
        "<!doctype html><title>Axon Watch Live</title><main>version-one marker-alpha</main>\n",
        encoding="utf-8",
    )
    (directory / "slow.html").write_text(
        "<!doctype html><title>Axon Watch Slow</title><main>slow version-one marker-alpha</main>\n",
        encoding="utf-8",
    )

    state_file = Path(args.log_file).resolve()
    state_file.parent.mkdir(parents=True, exist_ok=True)
    handler = lambda *h_args, **h_kwargs: FixtureHandler(
        *h_args,
        directory=str(directory),
        state_file=state_file,
        **h_kwargs,
    )
    with socketserver.TCPServer((args.host, args.port), handler) as server:
        port = server.server_address[1]
        Path(args.port_file).write_text(str(port), encoding="utf-8")
        print(f"fixture listening on {args.host}:{port}", file=sys.stderr, flush=True)
        server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Make the fixture executable**

Run:

```bash
chmod +x scripts/live-test-watch-fixture.py
```

- [ ] **Step 3: Verify the fixture compiles**

Run:

```bash
python3 -m py_compile scripts/live-test-watch-fixture.py
```

Expected: exit code `0`.

---

### Task 2: Add The Live Test Harness

**Files:**
- Create: `scripts/live-test-watch.sh`

- [ ] **Step 1: Write the harness**

Create `scripts/live-test-watch.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/axon-watch-live.XXXXXX")"
REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
AXON_BIN="${AXON_BIN:-$REPO/target/debug/axon}"
AXON_PORT="${AXON_PORT:-38181}"
AXON_BASE="http://127.0.0.1:${AXON_PORT}"
SQLITE="$ROOT/axon/jobs.db"
WATCH_NAME="live-watch-$(date +%s)"
WATCH_URL="${AXON_WATCH_LIVE_URL:-}"
FIXTURE_DIR="${AXON_WATCH_FIXTURE_DIR:-}"
MUTATE_CMD="${AXON_WATCH_MUTATE_CMD:-}"
SLOW_URL="${AXON_WATCH_SLOW_URL:-}"

cleanup() {
  set +e
  if [ -n "${AXON_PID:-}" ]; then kill "$AXON_PID" 2>/dev/null || true; wait "$AXON_PID" 2>/dev/null || true; fi
  if [ -n "${FIXTURE_PID:-}" ]; then kill "$FIXTURE_PID" 2>/dev/null || true; wait "$FIXTURE_PID" 2>/dev/null || true; fi
  echo "evidence root: $ROOT"
}
trap cleanup EXIT

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing dependency: $1" >&2; exit 2; }
}
need curl
need jq
need sqlite3
need python3

if [ ! -x "$AXON_BIN" ]; then
  cargo build --bin axon
fi

if [ -z "$WATCH_URL" ]; then
  mkdir -p "$ROOT/fixture"
  python3 "$REPO/scripts/live-test-watch-fixture.py" \
    --directory "$ROOT/fixture" \
    --port-file "$ROOT/fixture.port" \
    --log-file "$ROOT/fixture.log" &
  FIXTURE_PID=$!
  for _ in $(seq 1 50); do [ -s "$ROOT/fixture.port" ] && break; sleep 0.1; done
  LOCAL_PORT="$(cat "$ROOT/fixture.port")"
  echo "local fixture: http://127.0.0.1:${LOCAL_PORT}/index.html" >&2
  echo "production-mode Axon rejects loopback/private watch URLs." >&2
  echo "Expose this fixture through a public hostname and rerun with:" >&2
  echo "  AXON_WATCH_LIVE_URL=https://public.example.test/index.html" >&2
  echo "  AXON_WATCH_FIXTURE_DIR=$ROOT/fixture" >&2
  echo "  AXON_WATCH_MUTATE_CMD='printf ... > $ROOT/fixture/index.html'" >&2
  exit 2
fi

if [ -n "$FIXTURE_DIR" ] && [ -z "$MUTATE_CMD" ]; then
  MUTATE_CMD="printf '%s\n' '<!doctype html><title>Axon Watch Live</title><main>version-two marker-beta live-change</main>' > '$FIXTURE_DIR/index.html'"
fi

mkdir -p "$ROOT/axon"
cat > "$ROOT/axon/config.toml" <<'TOML'
[workers]
watchdog-stale-timeout-secs = 60
watchdog-confirm-secs = 5
TOML

AXON_HOME="$ROOT/axon" \
AXON_DATA_DIR="$ROOT/axon" \
AXON_SQLITE_PATH="$SQLITE" \
AXON_CONFIG_PATH="$ROOT/axon/config.toml" \
AXON_MCP_HTTP_HOST=127.0.0.1 \
AXON_MCP_HTTP_PORT="$AXON_PORT" \
AXON_WATCH_TICK_SECS=1 \
AXON_WATCH_LEASE_SECS=45 \
"$AXON_BIN" --skip-embed serve >"$ROOT/axon.log" 2>&1 &
AXON_PID=$!

for _ in $(seq 1 100); do
  if curl -fsS "$AXON_BASE/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.2
done
curl -fsS "$AXON_BASE/healthz" >/dev/null

now_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
watch_json="$(curl -fsS -X POST "$AXON_BASE/v1/watch" \
  -H 'Content-Type: application/json' \
  -d "$(jq -nc --arg name "$WATCH_NAME" --arg url "$WATCH_URL" --arg next "$now_utc" '{
    name: $name,
    task_type: "watch",
    task_payload: {
      urls: [$url],
      summarize: false,
      max_depth: 1,
      change_threshold_words: 0
    },
    every_seconds: 30,
    enabled: true,
    next_run_at: $next
  }')")"
watch_id="$(jq -r '.id' <<<"$watch_json")"
echo "watch_id=$watch_id"

sql_count() {
  sqlite3 "$SQLITE" "$1"
}

wait_sql() {
  local label="$1"
  local query="$2"
  local expected="$3"
  local timeout="${4:-60}"
  local start now got
  start="$(date +%s)"
  while true; do
    got="$(sql_count "$query" || true)"
    if [ "$got" = "$expected" ]; then
      echo "ok: $label -> $got"
      return 0
    fi
    now="$(date +%s)"
    if [ $((now - start)) -ge "$timeout" ]; then
      echo "timeout: $label expected $expected got $got" >&2
      echo "--- axon log ---" >&2
      tail -200 "$ROOT/axon.log" >&2 || true
      exit 1
    fi
    sleep 1
  done
}

wait_sql "first completed watch run" "SELECT COUNT(*) FROM axon_watch_runs WHERE watch_id='$watch_id' AND status='completed';" "1" 75
wait_sql "first crawl job enqueued" "SELECT COUNT(*) FROM axon_crawl_jobs;" "1" 30
wait_sql "first artifact written" "SELECT COUNT(*) FROM axon_watch_run_artifacts;" "1" 30
first_changed="$(sql_count "SELECT json_extract(result_json, '$.changed') FROM axon_watch_runs WHERE watch_id='$watch_id' ORDER BY created_at DESC LIMIT 1;")"
[ "$first_changed" = "1" ] || { echo "expected first run changed=1 got $first_changed" >&2; exit 1; }

wait_sql "second completed watch run" "SELECT COUNT(*) FROM axon_watch_runs WHERE watch_id='$watch_id' AND status='completed';" "2" 60
second_changed="$(sql_count "SELECT json_extract(result_json, '$.changed') FROM axon_watch_runs WHERE watch_id='$watch_id' ORDER BY created_at DESC LIMIT 1;")"
[ "$second_changed" = "0" ] || { echo "expected second run changed=0 got $second_changed" >&2; exit 1; }
second_crawls="$(sql_count "SELECT COUNT(*) FROM axon_crawl_jobs;")"
[ "$second_crawls" = "1" ] || { echo "unchanged run enqueued duplicate crawl count=$second_crawls" >&2; exit 1; }
second_artifacts="$(sql_count "SELECT COUNT(*) FROM axon_watch_run_artifacts;")"
[ "$second_artifacts" = "1" ] || { echo "unchanged run wrote duplicate artifact count=$second_artifacts" >&2; exit 1; }

if [ -z "$MUTATE_CMD" ]; then
  echo "AXON_WATCH_MUTATE_CMD is required to mutate the public fixture" >&2
  exit 2
fi
sleep 2
bash -lc "$MUTATE_CMD"

wait_sql "third completed watch run" "SELECT COUNT(*) FROM axon_watch_runs WHERE watch_id='$watch_id' AND status='completed';" "3" 75
third_changed="$(sql_count "SELECT json_extract(result_json, '$.changed') FROM axon_watch_runs WHERE watch_id='$watch_id' ORDER BY created_at DESC LIMIT 1;")"
[ "$third_changed" = "1" ] || { echo "expected third run changed=1 got $third_changed" >&2; exit 1; }
wait_sql "second crawl job enqueued after mutation" "SELECT COUNT(*) FROM axon_crawl_jobs;" "2" 30
wait_sql "second artifact written after mutation" "SELECT COUNT(*) FROM axon_watch_run_artifacts;" "2" 30

"$AXON_BIN" --json watch list >"$ROOT/watch-list.json"
"$AXON_BIN" --json watch history "$watch_id" --limit 5 >"$ROOT/watch-history.json"
latest_run_id="$(jq -r '.[0].id' "$ROOT/watch-history.json")"
"$AXON_BIN" --json watch artifacts "$latest_run_id" --limit 5 >"$ROOT/watch-artifacts.json"

if [ -n "$SLOW_URL" ]; then
  slow_next="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  slow_json="$(curl -fsS -X POST "$AXON_BASE/v1/watch" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg name "${WATCH_NAME}-slow" --arg url "$SLOW_URL" --arg next "$slow_next" '{
      name: $name,
      task_type: "watch",
      task_payload: { urls: [$url], summarize: false, max_depth: 1, change_threshold_words: 0 },
      every_seconds: 30,
      enabled: true,
      next_run_at: $next
    }')")"
  slow_watch_id="$(jq -r '.id' <<<"$slow_json")"
  sleep 2
  set +e
  manual_out="$(curl -fsS -X POST "$AXON_BASE/v1/watch/$slow_watch_id/run" 2>&1)"
  manual_status=$?
  set -e
  [ "$manual_status" -ne 0 ] || { echo "expected manual run-now to fail while scheduled slow run is leased; got $manual_out" >&2; exit 1; }
fi

jq -n \
  --arg root "$ROOT" \
  --arg watch_id "$watch_id" \
  --arg first_changed "$first_changed" \
  --arg second_changed "$second_changed" \
  --arg third_changed "$third_changed" \
  --arg crawls "$(sql_count "SELECT COUNT(*) FROM axon_crawl_jobs;")" \
  --arg artifacts "$(sql_count "SELECT COUNT(*) FROM axon_watch_run_artifacts;")" \
  '{root:$root, watch_id:$watch_id, first_changed:$first_changed, second_changed:$second_changed, third_changed:$third_changed, crawls:$crawls, artifacts:$artifacts}'
```

- [ ] **Step 2: Make the harness executable**

Run:

```bash
chmod +x scripts/live-test-watch.sh
```

- [ ] **Step 3: Verify shell syntax**

Run:

```bash
bash -n scripts/live-test-watch.sh
```

Expected: exit code `0`.

---

### Task 3: Run The Production-Mode Live Test

**Files:**
- No code changes.
- Uses: `scripts/live-test-watch.sh`

- [ ] **Step 1: Expose a controlled fixture URL**

Start the fixture locally:

```bash
ROOT="$(mktemp -d /tmp/axon-watch-fixture.XXXXXX)"
mkdir -p "$ROOT/site"
python3 scripts/live-test-watch-fixture.py \
  --directory "$ROOT/site" \
  --port-file "$ROOT/fixture.port" \
  --log-file "$ROOT/fixture.log" &
echo $! > "$ROOT/fixture.pid"
cat "$ROOT/fixture.port"
```

Expose `http://127.0.0.1:$(cat "$ROOT/fixture.port")/index.html` through a public/non-private hostname. Do not use `127.0.0.1`, a Tailscale IP, or an RFC-1918 LAN address as the watched URL; production `validate_url()` should reject those.

Example expected watched URL shape:

```text
https://axon-watch-live-test.example.com/index.html
```

- [ ] **Step 2: Run the full live test**

Run:

```bash
AXON_WATCH_LIVE_URL="https://axon-watch-live-test.example.com/index.html" \
AXON_WATCH_SLOW_URL="https://axon-watch-live-test.example.com/slow.html" \
AXON_WATCH_FIXTURE_DIR="$ROOT/site" \
AXON_WATCH_MUTATE_CMD="printf '%s\n' '<!doctype html><title>Axon Watch Live</title><main>version-two marker-beta live-change</main>' > '$ROOT/site/index.html'" \
./scripts/live-test-watch.sh
```

Expected evidence:

```text
watch_id=<uuid>
ok: first completed watch run -> 1
ok: first crawl job enqueued -> 1
ok: first artifact written -> 1
ok: second completed watch run -> 2
ok: third completed watch run -> 3
ok: second crawl job enqueued after mutation -> 2
ok: second artifact written after mutation -> 2
{
  "first_changed": "1",
  "second_changed": "0",
  "third_changed": "1",
  "crawls": "2",
  "artifacts": "2"
}
```

- [ ] **Step 3: Inspect fixture conditional-probe evidence**

Run:

```bash
jq -c 'select(.if_modified_since != null or .if_none_match != null)' "$ROOT/fixture.log" | tail -20
```

Expected: at least one request after the initial seed contains `if_modified_since` or `if_none_match`. A `304` is ideal when the proxy preserves validators; a `200` followed by `changed=0` is still acceptable because Axon falls back to full diff confirmation.

- [ ] **Step 4: Archive the evidence**

Run:

```bash
find /tmp -maxdepth 1 -type d -name 'axon-watch-live.*' -mtime -1 -print
```

Expected: the test prints its evidence root. Preserve:

```text
axon.log
watch-list.json
watch-history.json
watch-artifacts.json
jobs.db
fixture.log
```

---

### Task 4: Document The Live Test

**Files:**
- Modify: `docs/operations/operations.md`

- [ ] **Step 1: Add the operations section**

Add this section to `docs/operations/operations.md`:

````markdown
## Watch Live Operating Test

Use `scripts/live-test-watch.sh` to prove the watch scheduler works end-to-end in server mode. This test requires a controlled mutable page behind a public/non-private URL because production `validate_url()` rejects loopback, LAN, link-local, and Tailscale/private addresses.

Minimum command:

```bash
AXON_WATCH_LIVE_URL="https://axon-watch-live-test.example.com/index.html" \
AXON_WATCH_FIXTURE_DIR="/path/to/public/fixture/root" \
AXON_WATCH_MUTATE_CMD="printf '%s\n' '<!doctype html><main>version-two marker-beta</main>' > /path/to/public/fixture/root/index.html" \
./scripts/live-test-watch.sh
````

The test passes only when it proves:

- first scheduled watch run completes with `changed=1`
- first run writes one `url-change` artifact
- first run enqueues one crawl
- second scheduled run on unchanged content completes with `changed=0`
- unchanged run does not enqueue a new crawl or artifact
- mutating the page causes the next scheduled run to complete with `changed=1`
- changed run writes one additional artifact and enqueues exactly one additional crawl
- CLI `watch list`, `watch history`, and `watch artifacts` can read the same evidence
```

- [ ] **Step 2: Verify docs mention the SSRF constraint**

Run:

```bash
rg -n "Watch Live Operating Test|public/non-private URL|validate_url" docs/operations/operations.md
```

Expected: all three phrases are found.

---

### Task 5: Final Verification

**Files:**
- Verify: `scripts/live-test-watch.sh`
- Verify: `scripts/live-test-watch-fixture.py`
- Verify: `docs/operations/operations.md`

- [ ] **Step 1: Run static verification**

Run:

```bash
bash -n scripts/live-test-watch.sh
python3 -m py_compile scripts/live-test-watch-fixture.py
```

Expected: both commands exit `0`.

- [ ] **Step 2: Run targeted Rust regression tests**

Run:

```bash
cargo test --lib watch -- --nocapture
```

Expected: all filtered watch tests pass.

- [ ] **Step 3: Run the live operating test**

Run the command from Task 3 with a real public fixture URL.

Expected: the evidence JSON reports `first_changed=1`, `second_changed=0`, `third_changed=1`, `crawls=2`, and `artifacts=2`.

- [ ] **Step 4: Commit**

Run:

```bash
git add scripts/live-test-watch.sh scripts/live-test-watch-fixture.py docs/operations/operations.md docs/superpowers/plans/2026-06-07-watch-full-live-operating-test.md
git commit -m "test: add watch live operating harness plan"
```

Expected: commit succeeds with only the planned files staged.

---

## Self-Review

- Spec coverage: The plan covers production URL validation, REST create, scheduler auto-fire, manual run-now guard, diff-gated unchanged skip, changed recrawl dispatch, artifacts, history, CLI inspection, SQLite evidence, and cleanup.
- Placeholder scan: No `TBD`, `TODO`, or unspecified test assertions remain.
- Type consistency: The plan uses the real watch payload fields (`urls`, `summarize`, `max_depth`, `change_threshold_words`) and the real CLI subcommands (`watch list`, `watch history`, `watch artifacts`).
