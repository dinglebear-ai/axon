#!/usr/bin/env python3
"""Build a tiny synthetic supported-upgrade database from digest-pinned SQL."""
from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = ROOT / "tests/e2e/fixtures/upgrades/epoch1-jobs-v5.json"
NOW = "2026-01-01T00:00:00Z"
REFRESH_FIXTURE = ROOT / "tests/e2e/fixtures/upload/document.md"
HISTORICAL_JOB_ID = "11111111-1111-4111-8111-111111111111"
HISTORICAL_ARTIFACT_ID = "art_upgrade_record"


class FixtureError(RuntimeError):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def refresh_identity() -> tuple[str, str]:
    canonical = REFRESH_FIXTURE.resolve().as_uri()
    token = digest(canonical.encode())[:24]
    return f"src_local_{token}", f"local://{token}"


def load_manifest(path: Path) -> dict:
    value = json.loads(path.read_text())
    required = {"schema_version", "fixture_id", "source", "target", "support_policy", "generator",
                "explicit_collection_migration", "migrations"}
    missing = required - value.keys()
    if missing:
        raise FixtureError(f"fixture provenance missing fields: {sorted(missing)}")
    if value["schema_version"] != 1 or not value["source"].get("artifact_digest"):
        raise FixtureError("fixture provenance has an unsupported schema or missing artifact digest")
    if value["generator"] != {"path": "scripts/e2e/build-upgrade-fixture.py", "version": 1}:
        raise FixtureError("fixture generator drift: expected build-upgrade-fixture.py version 1")
    return value


def verified_inputs(manifest: dict) -> list[tuple[dict, bytes]]:
    inputs = []
    for item in manifest["migrations"]:
        path = (ROOT / item["path"]).resolve()
        if ROOT not in path.parents or not path.is_file():
            raise FixtureError(f"invalid migration path: {item['path']}")
        data = path.read_bytes()
        actual = digest(data)
        if actual != item["sha256"]:
            raise FixtureError(f"fixture input drift: {item['path']} expected {item['sha256']} got {actual}")
        inputs.append((item, data))
    artifact = digest(b"".join(data for _, data in inputs))
    expected = manifest["source"]["artifact_digest"].removeprefix("sha256:")
    if artifact != expected:
        raise FixtureError(f"fixture artifact drift: expected {expected} got {artifact}")
    return inputs


def seed(conn: sqlite3.Connection, fixture_id: str) -> None:
    source_id, canonical_uri = refresh_identity()
    conn.execute("PRAGMA foreign_keys=ON")
    summary = {"source_id": source_id, "canonical_uri": canonical_uri, "display_name": "document.md",
               "source_kind": "local", "adapter": {"name": "local", "version": "1"},
               "authority": "user_pinned", "status": "completed",
               "counts": {"items_total": 1, "items_changed": 1, "documents_total": 1,
                          "chunks_total": 1, "vector_points_total": 0, "bytes_total": 59},
               "created_at": NOW, "updated_at": NOW, "tags": [fixture_id]}
    conn.execute("INSERT INTO sources(source_id,summary_json,created_at,updated_at) VALUES(?,?,?,?)",
                 (source_id, json.dumps(summary), NOW, NOW))
    conn.execute("INSERT INTO source_generations(source_id,generation,sequence,status,publish_state,generation_json,created_at,published_at) VALUES(?,?,?,?,?,?,?,?)",
                 (source_id, "generation-upgrade", 1, "committed", "published", '{"synthetic":true}', NOW, NOW))
    conn.execute("INSERT INTO source_manifests(source_id,generation,manifest_json,created_at) VALUES(?,?,?,?)",
                 (source_id, "generation-upgrade", json.dumps({"items": ["document.md"]}), NOW))
    conn.execute("INSERT INTO source_items(source_id,source_item_key,generation,item_canonical_uri,content_hash,item_json) VALUES(?,?,?,?,?,?)",
                 (source_id, "document.md", "generation-upgrade", REFRESH_FIXTURE.resolve().as_uri(), "sha256:synthetic", json.dumps({"path": "document.md"})))
    conn.execute("INSERT INTO document_status(document_id,source_id,source_item_key,generation,status,status_json,updated_at) VALUES(?,?,?,?,?,?,?)",
                 ("document-upgrade", source_id, "document.md", "generation-upgrade", "published", '{"synthetic":true}', NOW))
    conn.execute("INSERT INTO axon_source_watches(watch_id,source,source_id,canonical_uri,adapter_name,adapter_version,scope,embed,options_json,collection,enabled,every_seconds,next_run_at,created_at,updated_at,auth_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                 ("watch-upgrade", str(REFRESH_FIXTURE.resolve()), source_id, canonical_uri, "local", "1", "file", 0, "{}", "axon_e2e_upgrade", 1, 3600, 1893456000000, 1767225600000, 1767225600000, '{}'))
    conn.execute("INSERT INTO jobs(job_id,kind,status,phase,priority,source_id,watch_id,created_at,updated_at,last_event_sequence) VALUES(?,?,?,?,?,?,?,?,?,?)",
                 (HISTORICAL_JOB_ID, "source", "completed", "complete", "normal", source_id, "watch-upgrade", NOW, NOW, 1))
    conn.execute("INSERT INTO job_attempts(attempt_id,job_id,attempt,status,started_at,finished_at) VALUES(?,?,?,?,?,?)",
                 ("attempt-upgrade", HISTORICAL_JOB_ID, 0, "completed", NOW, NOW))
    conn.execute("INSERT INTO job_events(event_id,job_id,sequence,attempt,phase,status,severity,visibility,message,timestamp) VALUES(?,?,?,?,?,?,?,?,?,?)",
                 ("event-upgrade", HISTORICAL_JOB_ID, 1, 0, "complete", "completed", "info", "public", "synthetic historical completion", NOW))
    conn.execute("INSERT INTO job_artifacts(artifact_id,job_id,artifact_kind,uri,created_at) VALUES(?,?,?,?,?)",
                 (HISTORICAL_ARTIFACT_ID, HISTORICAL_JOB_ID, "document", "fixture://document.md", NOW))
    conn.execute("INSERT INTO axon_source_watch_runs(watch_id,job_id,created_at) VALUES(?,?,?)",
                 ("watch-upgrade", HISTORICAL_JOB_ID, 1767225600000))
    conn.execute("INSERT INTO axon_observe_events(event_id,job_id,sequence,phase,status,severity,visibility,message,timestamp,event_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                 ("observe-event-upgrade", HISTORICAL_JOB_ID, 1, "complete", "completed", "info", "public",
                  "synthetic observed completion", NOW, '{"synthetic":true}', 1767225600000))
    conn.execute("INSERT INTO graph_nodes(node_id,kind,stable_key,canonical_uri,display_name,authority,confidence,metadata_json,source_ids_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                 ("node-upgrade", "document", "document.md", REFRESH_FIXTURE.resolve().as_uri(), "Synthetic document", "fixture", 1.0, "{}", json.dumps([source_id]), NOW, NOW))
    conn.execute("INSERT INTO graph_publication_state(source_id,committed_epoch,updated_at) VALUES(?,?,?)",
                 (source_id, 1, NOW))
    conn.execute("INSERT INTO memory_records(memory_id,memory_type,status,body,confidence,salience,scope_kind,scope_value,history_json,embedding_refs_json,visibility,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
                 ("memory-upgrade", "fact", "active", "Synthetic upgrade memory", 0.9, 0.8, "global", "*", "[]", "[]", "internal", NOW, NOW))
    conn.execute("INSERT INTO memory_links(memory_id,link_type,target,confidence,evidence_json,created_at) VALUES(?,?,?,?,?,?)",
                 ("memory-upgrade", "source", source_id, 1.0, "[]", NOW))


def build(manifest_path: Path, output: Path) -> dict:
    manifest = load_manifest(manifest_path)
    inputs = verified_inputs(manifest)
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()
    conn = sqlite3.connect(output)
    try:
        conn.execute("PRAGMA user_version=1")
        conn.execute("CREATE TABLE axon_applied_migrations(namespace TEXT NOT NULL,version INTEGER NOT NULL,name TEXT NOT NULL,checksum TEXT NOT NULL,schema_epoch INTEGER NOT NULL,applied_at TEXT NOT NULL DEFAULT (datetime('now')),PRIMARY KEY(namespace,version))")
        for item, data in inputs:
            conn.executescript(data.decode())
            conn.execute("INSERT INTO axon_applied_migrations(namespace,version,name,checksum,schema_epoch,applied_at) VALUES(?,?,?,?,1,?)",
                         (item["namespace"], item["version"], item["name"], digest(data), NOW))
        seed(conn, manifest["fixture_id"])
        conn.commit()
    finally:
        conn.close()
    result = {"fixture_id": manifest["fixture_id"], "database": str(output), "sha256": digest(output.read_bytes()),
              "source_version": manifest["source"]["version"], "source_schema_epoch": 1,
              "migration_receipts": len(inputs), "synthetic": True,
              "refresh_source_id": refresh_identity()[0], "refresh_fixture": str(REFRESH_FIXTURE.resolve())}
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = build(args.manifest, args.output)
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.report:
        args.report.write_text(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
