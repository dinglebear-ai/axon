#!/usr/bin/env python3
"""Exercise supported persisted-state upgrades with the current Axon binary."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/e2e/fixtures/upgrades/epoch1-jobs-v5.json"


def load_builder():
    path = Path(__file__).with_name("build-upgrade-fixture.py")
    spec = importlib.util.spec_from_file_location("axon_upgrade_builder", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("upgrade fixture builder unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


builder = load_builder()


def load_runtime(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"E2E runtime unavailable: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


isolation = load_runtime("axon_upgrade_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")
teardown = load_runtime("axon_upgrade_teardown", ROOT / "scripts/e2e/lib/teardown.py")


def register_outer_cleanup(manifest) -> None:
    registry = os.environ.get("AXON_E2E_CLEANUP_REGISTRY")
    if not registry: return
    report = manifest.path.parent / "outer-registry-registration.json"
    completed = subprocess.run([sys.executable, str(ROOT / "scripts/e2e/cleanup-owned-runs.py"),
        "--registry", registry, "--register-manifest", str(manifest.path), "--report", str(report)],
        cwd=ROOT, capture_output=True, text=True, timeout=15)
    if completed.returncode:
        raise RuntimeError(f"outer cleanup registration failed: {completed.stderr[:300]}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def query(path: Path, sql: str, params: tuple = ()) -> list[tuple]:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute(sql, params).fetchall()
    finally:
        conn.close()


def logical_digest(path: Path) -> str:
    """Hash schema and rows, ignoring harmless SQLite file-header churn."""
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        payload = {"user_version": conn.execute("PRAGMA user_version").fetchone()[0], "tables": {}}
        names = [row[0] for row in conn.execute(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        for name in names:
            quoted = name.replace('"', '""')
            payload["tables"][name] = conn.execute(f'SELECT * FROM "{quoted}" ORDER BY rowid').fetchall()
        return hashlib.sha256(json.dumps(payload, sort_keys=True, default=str, separators=(",", ":")).encode()).hexdigest()
    finally:
        conn.close()


def invoke(binary: Path, database: Path, data_dir: Path, expected: set[int] = {0}) -> subprocess.CompletedProcess:
    env = {**os.environ, "AXON_DATA_DIR": str(data_dir), "AXON_SQLITE_PATH": str(database),
           "QDRANT_URL": "", "TEI_URL": "", "AXON_CHROME_REMOTE_URL": ""}
    result = subprocess.run([str(binary), "status", "--json"], cwd=ROOT, env=env,
                            capture_output=True, text=True, timeout=30)
    if result.returncode not in expected:
        raise RuntimeError(f"Axon status exit {result.returncode}: {result.stderr}")
    return result


def wire_capabilities(binary: Path, database: Path, data_dir: Path) -> dict:
    env = {**os.environ, "AXON_DATA_DIR": str(data_dir), "AXON_SQLITE_PATH": str(database),
           "QDRANT_URL": "", "TEI_URL": "", "AXON_CHROME_REMOTE_URL": ""}
    completed = subprocess.run([str(binary), "capabilities", "--json"], cwd=ROOT, env=env,
                               capture_output=True, text=True, timeout=30)
    if completed.returncode:
        raise RuntimeError(f"current wire capabilities failed: {completed.stderr}")
    value = json.loads(completed.stdout)
    if value.get("schema_version") != "client-server.v1" or value.get("minimum_client_schema_version") != "client-server.v1":
        raise RuntimeError(f"unexpected declared compatibility window: {value}")
    return value


def current_binary_json(binary: Path, database: Path, data_dir: Path, arguments: list[str]) -> dict:
    env = {**os.environ, "AXON_DATA_DIR": str(data_dir), "AXON_SQLITE_PATH": str(database),
           "QDRANT_URL": "", "TEI_URL": "", "AXON_CHROME_REMOTE_URL": ""}
    completed = subprocess.run([str(binary), *arguments, "--json"], cwd=ROOT, env=env,
                               capture_output=True, text=True, timeout=30)
    if completed.returncode:
        raise RuntimeError(f"current binary {' '.join(arguments)} failed: {completed.stderr}")
    return json.loads(completed.stdout)


def exercise_domains(binary: Path, database: Path, data_dir: Path, source_id: str) -> dict:
    job = current_binary_json(binary, database, data_dir, ["jobs", "get", builder.HISTORICAL_JOB_ID])
    events = current_binary_json(binary, database, data_dir, ["jobs", "events", builder.HISTORICAL_JOB_ID])
    watch = current_binary_json(binary, database, data_dir, ["watch", "get", "watch-upgrade"])
    graph = current_binary_json(binary, database, data_dir, ["graph", "node", "node-upgrade", "--include-edges", "--include-evidence"])
    memory = current_binary_json(binary, database, data_dir, ["memory", "show", "memory-upgrade"])
    checks = {
        "job_get": job.get("job_id") == builder.HISTORICAL_JOB_ID and job.get("status") == "completed",
        "job_events": events.get("last_sequence") == 1 and events.get("events", [{}])[0].get("message") == "synthetic historical completion",
        "watch_get": watch.get("watch_id") == "watch-upgrade" and watch.get("source_id") == source_id,
        "graph_node": graph.get("node", {}).get("node_id") == "node-upgrade",
        "memory_show": memory.get("memory", {}).get("id") == "memory-upgrade",
        "artifact_job_coherence": query(database, "SELECT job_id FROM job_artifacts WHERE artifact_id=?", (builder.HISTORICAL_ARTIFACT_ID,)) == [(job.get("job_id"),)],
        "observe_job_coherence": query(database, "SELECT job_id FROM axon_observe_events WHERE event_id='observe-event-upgrade'") == [(job.get("job_id"),)],
    }
    failed = [name for name, passed in checks.items() if not passed]
    if failed:
        raise RuntimeError(f"current-binary domain operations failed: {failed}")
    return {"assertions": checks, "commands": ["jobs get", "jobs events", "watch get", "graph node", "memory show"],
            "artifact_note": "job_artifacts has no standalone transport route; coherence is proven against the current-binary job identity",
            "observability_note": "observe tables are internal durable diagnostics; coherence is proven against the current-binary job identity"}


def incompatible_copy(source: Path, target: Path, mutation: str) -> None:
    shutil.copy2(source, target)
    conn = sqlite3.connect(target)
    try:
        if mutation == "forward_epoch":
            conn.execute("PRAGMA user_version=2")
        elif mutation == "receipt_tamper":
            conn.execute("UPDATE axon_applied_migrations SET checksum='tampered' WHERE namespace='jobs' AND version=5")
        elif mutation == "partial_fixture":
            conn.execute("DROP TABLE graph_nodes")
        elif mutation == "interrupted":
            conn.execute("CREATE TABLE embedding_vector_cache(unexpected TEXT)")
        else:
            raise ValueError(mutation)
        conn.commit()
    finally:
        conn.close()


def assert_semantics(database: Path, source_id: str) -> dict[str, object]:
    assertions = {
        "historical_job": query(database, "SELECT status FROM jobs WHERE job_id=?", (builder.HISTORICAL_JOB_ID,)) == [("completed",)],
        "historical_event": query(database, "SELECT message FROM job_events WHERE event_id='event-upgrade'") == [("synthetic historical completion",)],
        "historical_artifact": query(database, "SELECT uri FROM job_artifacts WHERE artifact_id=?", (builder.HISTORICAL_ARTIFACT_ID,)) == [("fixture://document.md",)],
        "historical_watch": query(database, "SELECT source_id FROM axon_source_watches WHERE watch_id='watch-upgrade'") == [(source_id,)],
        "ledger_identity": query(database, "SELECT generation FROM source_generations WHERE source_id=?", (source_id,)) == [("generation-upgrade",)],
        "graph_record": query(database, "SELECT stable_key FROM graph_nodes WHERE node_id='node-upgrade'") == [("document.md",)],
        "memory_record": query(database, "SELECT body FROM memory_records WHERE memory_id='memory-upgrade'") == [("Synthetic upgrade memory",)],
        "observability_record": query(database, "SELECT message FROM axon_observe_events WHERE event_id='observe-event-upgrade'") == [("synthetic observed completion",)],
        "schema_epoch": query(database, "PRAGMA user_version") == [(1,)],
        "tail_migration": query(database, "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace='jobs' AND version=10") == [(1,)],
    }
    failed = [name for name, passed in assertions.items() if not passed]
    if failed:
        raise RuntimeError(f"post-upgrade semantic assertions failed: {failed}")
    return assertions


def refresh(binary: Path, database: Path, data_dir: Path, fixture: Path) -> dict:
    env = {**os.environ, "AXON_DATA_DIR": str(data_dir), "AXON_SQLITE_PATH": str(database),
           "QDRANT_URL": "", "TEI_URL": "", "AXON_CHROME_REMOTE_URL": ""}
    results = []; graph_counts = []
    for _ in range(2):
        completed = subprocess.run([str(binary), "source", str(fixture), "--skip-embed", "--wait", "true", "--json"],
                                   cwd=ROOT, env=env, capture_output=True, text=True, timeout=30)
        if completed.returncode:
            raise RuntimeError(f"post-upgrade refresh failed: {completed.stderr}")
        results.append(json.loads(completed.stdout))
        graph_counts.append(query(database, "SELECT COUNT(*),COUNT(DISTINCT node_id) FROM graph_nodes")[0])
    source_id = results[0]["source_id"]
    if any(result["source_id"] != source_id or result["status"] not in {"completed", "completed_degraded"} for result in results):
        raise RuntimeError("post-upgrade refresh changed identity or failed")
    generations = query(database, "SELECT generation,sequence,status FROM source_generations WHERE source_id=? ORDER BY sequence", (source_id,))
    if len(generations) < 2 or query(database, "SELECT COUNT(*) FROM sources WHERE source_id=?", (source_id,)) != [(1,)]:
        raise RuntimeError("post-upgrade refresh did not preserve one source and advance generation state")
    if graph_counts[1] != graph_counts[0] or graph_counts[1][0] != graph_counts[1][1]:
        raise RuntimeError(f"idempotent refresh duplicated graph publication: {graph_counts}")
    return {"source_id": source_id, "runs": results, "generations": [list(row) for row in generations],
            "vector_points_expected": 0, "vector_policy": "skip-embed fixture; zero publication is consistent",
            "graph_node_counts_after_refresh": [list(row) for row in graph_counts], "duplicate_graph_nodes": 0}


def run(binary: Path, output: Path, *, failure_at: str | None = None,
        hold_seconds: float = 0, run_root_record: Path | None = None,
        manifest_root: Path | None = None) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    disposable = output / "disposable"
    evidence = output / "evidence"
    evidence.mkdir()
    manifest = None; cleanup = None; report = None; caught_error = None; previous_handlers = {}

    def interrupted(signum, _frame): raise InterruptedError(f"received signal {signum}")
    for signum in (signal.SIGINT, signal.SIGTERM): previous_handlers[signum] = signal.signal(signum, interrupted)
    try:
        disposable.mkdir()
        if run_root_record: run_root_record.write_text(str(disposable), encoding="utf-8")
        run_id = isolation.new_run_id(); data_dir = disposable / "data"; data_dir.mkdir()
        manifest = isolation.Manifest.create((manifest_root or (ROOT / "target/e2e/manifests")).resolve(), run_id, data_dir)
        register_outer_cleanup(manifest)
        manifest.register("temp_path", str(disposable)); manifest.register("data_dir", str(data_dir))
        if failure_at == "after_setup": raise RuntimeError("injected failure after setup")
        if hold_seconds: time.sleep(hold_seconds)
        fixture_manifest = builder.load_manifest(MANIFEST)
        database = data_dir / "jobs.db"; manifest.register("sqlite", str(database))
        build_report = builder.build(MANIFEST, database)
        source_id = build_report["refresh_source_id"]
        backup = data_dir / "jobs.db.pre-upgrade"; manifest.register("sqlite", str(backup))
        shutil.copy2(database, backup)
        backup_digest = sha(backup)
        before_receipts = query(database, "SELECT namespace,version,name FROM axon_applied_migrations ORDER BY namespace,version")
        invoke(binary, database, data_dir)
        first_digest = sha(database)
        semantics = assert_semantics(database, source_id)
        invoke(binary, database, data_dir)
        semantics_after_reopen = assert_semantics(database, source_id)
        refresh_report = refresh(binary, database, data_dir, Path(build_report["refresh_fixture"]))
        if refresh_report["source_id"] != source_id:
            raise RuntimeError(f"post-upgrade source identity changed: {source_id} -> {refresh_report['source_id']}")
        migrations = query(database, "SELECT namespace,version,name FROM axon_applied_migrations ORDER BY namespace,version")
        capabilities = wire_capabilities(binary, database, data_dir)
        domain_operations = exercise_domains(binary, database, data_dir, source_id)
        added = [list(row) for row in migrations if row not in before_receipts]
        failures = {}
        for mutation in ("forward_epoch", "receipt_tamper", "partial_fixture", "interrupted"):
            candidate = data_dir / f"{mutation}.db"; manifest.register("sqlite", str(candidate))
            incompatible_copy(backup, candidate, mutation)
            before = logical_digest(candidate)
            result = invoke(binary, candidate, disposable / f"data-{mutation}", expected=set(range(1, 256)))
            message = result.stderr + result.stdout
            actionable = ("startup.incompatible_store" in message and "axon reset" in message)
            if mutation == "interrupted": actionable = "migration jobs/0007_embedding_vector_cache (v7) failed" in message
            if not actionable: raise RuntimeError(f"{mutation} lacked actionable rejection: {message}")
            if mutation != "interrupted" and logical_digest(candidate) != before: raise RuntimeError(f"{mutation} rejection mutated the database")
            if mutation == "interrupted" and query(candidate, "SELECT COUNT(*) FROM axon_applied_migrations") != [(len(before_receipts),)]:
                raise RuntimeError("interrupted migration committed partial receipts")
            failures[mutation] = {"exit": result.returncode, "actionable": True, "atomic": True}
        readonly = data_dir / "readonly.db"; manifest.register("sqlite", str(readonly))
        shutil.copy2(backup, readonly); readonly.chmod(0o400)
        readonly_result = invoke(binary, readonly, disposable / "readonly-data", expected=set(range(1, 256)))
        readonly.chmod(0o600)
        failures["insufficient_permissions"] = {"exit": readonly_result.returncode, "safe": sha(readonly) == backup_digest}
        if not failures["insufficient_permissions"]["safe"]: raise RuntimeError("read-only migration source changed")
        target_version = next(line.split('"')[1] for line in (ROOT / "Cargo.toml").read_text().splitlines()
                              if line.strip().startswith("version ="))
        report = {
        "schema_version": 1, "passed": True, "fixture": build_report,
        "source_version": build_report["source_version"], "target_version": target_version,
        "source_schema_epoch": 1, "target_schema_epoch": 1,
        "fixture_checksum": build_report["sha256"], "backup_checksum": backup_digest,
        "backup_integrity": sha(backup) == backup_digest, "migrations_applied": added,
        "semantic_assertions": semantics, "reopen_assertions": semantics_after_reopen,
        "current_binary_domain_operations": domain_operations,
        "source_refresh": refresh_report,
        "negative_assertions": failures,
        "wire_compatibility": {"accepted": [capabilities["schema_version"]], "excluded": ["client-server.v0"],
                               "current_binary_capabilities": capabilities,
                               "observed_rejection": False,
                               "reason": "Axon exposes capability-window declaration, not request-level schema negotiation; no older compatibility is promised"},
        "collection_migration": fixture_manifest["explicit_collection_migration"],
        "release_qualification": {"result": "pass", "consumer": "axon_rust-nnzde.23"},
        "database_changed_by_upgrade": first_digest != build_report["sha256"],
        }
    except BaseException as error:
        caught_error = error
    finally:
        for signum in previous_handlers: signal.signal(signum, signal.SIG_IGN)
        try:
            if manifest is not None: cleanup = teardown.Engine(manifest.path).run().json()
            else:
                shutil.rmtree(disposable, ignore_errors=True)
                cleanup = {"success": not disposable.exists(), "residual": [], "refused": [], "created": [], "removed": []}
        finally:
            for signum, handler in previous_handlers.items(): signal.signal(signum, handler)
        if disposable.exists():
            cleanup.setdefault("residual", []).append({"class":"run_root","reason":"disposable root remains"})
            cleanup["success"] = False
    if caught_error is not None: raise caught_error
    if not cleanup.get("success") or cleanup.get("residual") or cleanup.get("refused"):
        raise RuntimeError(f"upgrade teardown residual audit failed: {cleanup}")
    assert report is not None
    report_path = evidence / "upgrade-report.json"
    report["teardown"] = cleanup
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--failure-at", choices=("after_setup",), help=argparse.SUPPRESS)
    parser.add_argument("--test-hold-seconds", type=float, default=0, help=argparse.SUPPRESS)
    parser.add_argument("--run-root-record", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--manifest-root", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not args.binary.is_file():
        raise SystemExit(f"tested Axon binary not found: {args.binary}")
    # ``run`` owns creation of the output directory so it can fail closed when
    # a caller accidentally points it at pre-existing state.  The implicit
    # path must therefore be a fresh child of a temporary parent, not the
    # already-created directory returned by ``mkdtemp``.
    output = args.output or (Path(tempfile.mkdtemp(prefix="axon-e2e-upgrade-")) / "run")
    print(json.dumps(run(args.binary.resolve(), output.resolve(), failure_at=args.failure_at,
                         hold_seconds=args.test_hold_seconds, run_root_record=args.run_root_record,
                         manifest_root=args.manifest_root), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
