#!/usr/bin/env python3
"""Protocol-accurate Axon CLI double for orchestrator self-tests only."""

from __future__ import annotations

import hashlib
import json
import os
import sys
import time
import uuid
from pathlib import Path

data = Path(os.environ["AXON_DATA_DIR"])
data.mkdir(parents=True, exist_ok=True)
state_path = data / "fake-axon-state.json"
calls_path = Path(os.environ.get("AXON_E2E_FAKE_CALLS", data / "calls.jsonl"))
lock = data / ".fake-lock"


def acquire():
    for _ in range(500):
        try: lock.mkdir(); return
        except FileExistsError: time.sleep(.002)
    raise RuntimeError("fake state lock timeout")


def load():
    if not state_path.exists(): return {"jobs": {}, "sources": {}, "artifacts": {}}
    return json.loads(state_path.read_text())


def save(state): state_path.write_text(json.dumps(state, sort_keys=True))
def emit(value, code=0): print(json.dumps(value)); raise SystemExit(code)
def value_after(args, name, default=None):
    return args[args.index(name)+1] if name in args else default


args = sys.argv[1:]
with calls_path.open("a") as handle: handle.write(json.dumps(args) + "\n")
if args == ["doctor", "--json"]: emit({"all_ok": True, "services": {"qdrant": {"ok": True}, "tei": {"ok": True}, "chrome": {"ok": True}}})
if args[:2] == ["jobs", "worker"]:
    while True: time.sleep(1)

acquire()
try:
    state = load()
    if args and args[0] == "source":
        source, scope = args[1], value_after(args, "--scope")
        if not source.startswith("http://127.0.0.1:32123/") and source.startswith((
                "http://169.254.", "http://[::1]", "http://127.",
                "http://localhost", "http://2852039166")):
            emit({"code": "source.ssrf_denied", "error": "unsafe source target"}, 2)
        wait = value_after(args, "--wait") == "true"
        collection = value_after(args, "--collection")
        source_id = "src_" + hashlib.sha256(source.encode()).hexdigest()[:12]
        job_id = "job_" + uuid.uuid4().hex
        transient = "transient" in source
        if Path(source).is_file(): digest = hashlib.sha256(Path(source).read_bytes()).hexdigest()
        else: digest = hashlib.sha256(source.encode()).hexdigest()
        previous = state["sources"].get(source_id)
        changed = not previous or previous["digest"] != digest
        generation = 1 if not previous else previous["generation"] + int(changed)
        state["sources"][source_id] = {"source_id": source_id, "source": source, "digest": digest,
                                       "generation": generation, "manifest_items": 1,
                                       "document_status": "published", "cleanup_debt": 0}
        blocked = any(marker in source for marker in ("block-fetch", "block-embed", "block-publish"))
        phase = "fetching" if "block-fetch" in source else "embedding" if "block-embed" in source else "publishing" if "block-publish" in source else "complete"
        status = "failed" if transient else ("running" if blocked else ("completed" if wait else "running"))
        state["jobs"][job_id] = {"job_id": job_id, "source_id": source_id, "source": source, "status": status,
                                     "generation": generation, "transient": transient, "polls": 0,
                                     "blocked": blocked, "phase": phase, "cleanup_debt_ids": []}
        artifact_id = "art_" + uuid.uuid4().hex
        state["artifacts"][job_id] = [] if transient else [{"artifact_id": artifact_id, "job_id": job_id, "kind": "raw_content"}]
        save(state)
        emit({"status": "completed" if wait and not transient else "queued", "job_id": job_id,
              "source_id": source_id, "canonical_uri": source, "collection": collection,
              "ledger": {"generation": f"gen_{generation}"},
              "counts": {"documents_total": 1, "chunks_total": 2,
                         "vector_points_total": 1 if changed and not transient else 0},
              "graph": {"nodes_upserted": 1, "edges_upserted": 1, "evidence_records": 1}})
    if args[:2] == ["jobs", "get"]:
        job = state["jobs"].get(args[2])
        if not job: emit({"error": "not found"}, 1)
        if job["status"] == "running" and job.get("phase") == "publishing":
            job["polls"] += 1
            if job["polls"] >= 2: job["status"] = "failed"
            save(state)
        elif job["status"] == "running" and not job.get("blocked"):
            job["polls"] += 1
            if job["polls"] >= 2: job["status"] = "completed"
            save(state)
        emit({"job_id": job["job_id"], "status": job["status"], "source_id": job["source_id"],
              "attempt": 2 if job.get("recovered") else 1,
              "counts": {"documents_done": 1, "chunks_done": 2}})
    if args[:2] == ["jobs", "list"]:
        emit({"items": list(state["jobs"].values()), "total": len(state["jobs"])})
    if args[:2] in (["jobs", "events"], ["jobs", "stream"]):
        job = state["jobs"][args[2]]
        events = [{"sequence": 1, "job_id": job["job_id"], "event": "source.prepared", "phase": job.get("phase", "complete"), "attempt": 1},
                  {"sequence": 2, "job_id": job["job_id"], "event": "adapter.chrome_rendered", "phase": "rendering", "attempt": 1}]
        if job.get("phase") == "publishing":
            events.append({"sequence": 3, "job_id": job["job_id"], "event": "source.partial_published",
                           "phase": "publishing", "attempt": 1})
        events.extend({"sequence": 3 + index, "job_id": job["job_id"], "event": "cleanup.debt_resolved", "phase": "cleaning", "attempt": 1, "debt_id": debt}
                      for index, debt in enumerate(job.get("cleanup_debt_ids", []) + job.get("resolved_cleanup_debt_ids", [])))
        if job.get("recovered"):
            events.append({"sequence": 100, "job_id": job["job_id"], "event": "jobs.recovered",
                           "phase": "complete", "attempt": 2})
        emit({"events": events,
              "last_sequence": 2})
    if args[:2] == ["jobs", "recover"]:
        recovered = 0
        for job in state["jobs"].values():
            if job["status"] == "running":
                job["status"] = "completed"; job["blocked"] = False; job["recovered"] = True; recovered += 1
        save(state)
        emit({"recovered": recovered, "scanned": len(state["jobs"]), "status": "completed"})
    if args[:2] == ["jobs", "cancel"]:
        job = state["jobs"][args[2]]
        if job["status"] not in {"completed", "failed"}: job["status"] = "canceled"
        side_effects = ["qdrant:partial_point"] if job.get("phase") == "publishing" else []
        debts = ["debt_" + uuid.uuid4().hex] if side_effects else []
        job["cleanup_debt_ids"] = debts
        save(state); emit({"job_id": job["job_id"], "status": job["status"],
                           "last_safe_stage": job.get("phase"), "side_effects": side_effects,
                           "cleanup_debt_ids": debts})
    if args[:2] == ["jobs", "retry"]:
        original = state["jobs"][args[2]]
        retry_id = original["job_id"]
        original.update({"status": "completed", "transient": False, "recovered": True})
        state["artifacts"][retry_id] = [{"artifact_id": "art_" + uuid.uuid4().hex, "job_id": retry_id, "kind": "raw_content"}]
        save(state); emit({"original_job_id": retry_id, "retry_job": {"id": retry_id, "status": "completed"}, "attempt": 2})
    if args[:2] == ["artifacts", "list"]:
        emit({"items": state["artifacts"].get(value_after(args, "--job-id"), [])})
    if args[:2] == ["graph", "query"]:
        linked = next((job["source_id"] for job in state["jobs"].values() if job["source"] == args[2]), "unknown")
        emit({"nodes": [{"id": "fixture-node", "source": args[2], "source_id": linked}], "edges": []})
    if args and args[0] == "query":
        emit({"results": [{"text": "Atlas deterministic fixture", "score": .99}]})
    if args and args[0] == "retrieve":
        text = "AXON_E2E_JS_ONLY_CONTENT" if "chrome-js" in args[1] else "Atlas deterministic fixture"
        emit({"chunks": [{"text": text, "source": args[1]}]})
    if args and args[0] == "sources":
        emit({"items": list(state["sources"].values())})
    if args and args[0] == "stats":
        emit({"collection": value_after(args, "--collection"), "source_count": len(state["sources"]),
              "vector_points": sum(item["generation"] for item in state["sources"].values()),
              "cleanup_debt": 0})
    if args[:2] == ["collections", "get"]:
        emit({"collection": args[2], "points_count": sum(item["generation"] for item in state["sources"].values())})
    if args[:2] == ["prune", "plan"]:
        debts = [debt for job in state["jobs"].values() if job.get("source_id") == args[2]
                 for debt in job.get("cleanup_debt_ids", [])]
        emit({"ok": True, "subaction": "plan", "plan": {"job_id": "plan_" + uuid.uuid4().hex,
              "selector": {"source_id": args[2]}, "cleanup_debt_ids": debts,
              "steps": [{"debt_id": debt, "kind": "vector_delete",
                         "selector": {"source_id": args[2]}} for debt in debts]}})
    if args[:2] == ["prune", "exec"]:
        for job in state["jobs"].values():
            job["resolved_cleanup_debt_ids"] = list(job.get("cleanup_debt_ids", []))
            job["cleanup_debt_ids"] = []
        save(state)
        emit({"ok": True, "subaction": "exec", "result": {"status": "completed",
              "cleanup_debt_remaining": 0}})
    if args[:2] == ["sync", "pending"]:
        emit({"status": "completed", "cleanup_debt": 0})
    emit({"error": "unsupported fake command", "args": args}, 2)
finally:
    if lock.exists(): lock.rmdir()
