#!/usr/bin/env python3
"""Authoritative, ownership-revalidating Axon E2E cleanup engine."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import signal
import socket
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol


def _load(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {filename}")
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module)
    return module


manifest_api = _load("axon_e2e_resource_manifest", "resource-manifest.py")
isolation = manifest_api.isolation
provider_api = _load("axon_e2e_provider_adapters", "provider-adapters.py")

PHASES = (
    ("quiesce", {"operation", "http_stream", "mcp_session", "chat_session"}),
    ("application", {"job_artifact", "job_event", "job_heartbeat", "job_stage", "job_attempt", "provider_reservation",
                     "config_snapshot", "job", "watch_run", "watch", "upload", "artifact", "document_status",
                     "cleanup_debt", "source_item", "source_manifest", "source_generation", "source_lease", "source",
                     "graph_evidence", "graph_conflict", "graph_edge", "graph_alias", "graph_node",
                     "memory_review", "memory_reinforcement", "memory_link", "memory_edge", "memory_record", "memory_node",
                     "observe_event", "observe_heartbeat", "observe_provider_health", "evidence", "auth_session", "token"}),
    ("provider-children", {"payload_index", "point", "qdrant_alias", "qdrant_snapshot"}),
    ("provider-collections", {"collection"}),
    ("processes", {"process"}),
    ("containers", {"compose_project", "container", "network", "volume"}),
    ("tailscale", {"tailscale_node"}),
    ("leases", {"port", "lease", "lock"}),
    ("files", {"cache", "chrome_diagnostic", "chrome_profile", "credential_file", "download", "feed_fixture",
               "git_fixture", "http_cache", "output", "screenshot", "socket", "sqlite", "sqlite_sidecar",
               "temp_path", "warc"}),
    ("run-root", {"data_dir"}),
)
PROVIDER_TYPES = set().union(*(types for name, types in PHASES if name not in {"processes", "leases", "files", "run-root"}))


def canonical_state_digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


class CleanupError(RuntimeError):
    pass


class Adapter(Protocol):
    def marker(self, resource: Any) -> dict[str, Any] | None: ...
    def delete(self, resource: Any, deadline: float) -> str: ...
    def exists(self, resource: Any) -> bool | None: ...


class RefusingAdapter:
    def marker(self, resource: Any) -> None: return None
    def delete(self, resource: Any, deadline: float) -> str:
        raise CleanupError("no exact provider adapter configured")
    def exists(self, resource: Any) -> None: return None


class LocalAdapter:
    """Exact local cleanup; never follows manifest paths outside the owned run."""

    def __init__(self, header: Any): self.header = header
    def marker(self, resource: Any) -> dict[str, Any]:
        return manifest_api.provider_marker(self.header, resource)

    def _owned_path(self, resource: Any) -> Path:
        path = Path(resource.identity).resolve()
        root = self.header.data_dir.parent.resolve()
        if path != root and root not in path.parents:
            raise CleanupError("local path is outside the owned run root")
        return path

    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() > deadline: raise TimeoutError("phase deadline exceeded")
        if resource.resource_type == "process": return self._process(resource, deadline)
        if resource.resource_type == "port":
            lease = Path(str(resource.metadata.get("lease", ""))).resolve()
            owner_file = lease / "owner.json"
            try: owner = json.loads(owner_file.read_text())["run_id"]
            except (OSError, KeyError, json.JSONDecodeError) as error:
                if not lease.exists(): return "absent"
                raise CleanupError("port lease ownership is unknown") from error
            if owner != self.header.run_id: raise CleanupError("port lease ownership changed")
            shutil.rmtree(lease); return "removed"
        if resource.resource_type in {"cache", "chrome_diagnostic", "chrome_profile", "credential_file", "data_dir",
                                      "download", "feed_fixture", "git_fixture", "http_cache", "lease", "lock", "output",
                                      "screenshot", "socket", "sqlite", "sqlite_sidecar", "temp_path", "warc"}:
            path = self._owned_path(resource)
            if resource.resource_type == "sqlite":
                removed = False
                for suffix in ("", "-wal", "-shm", "-journal"):
                    candidate = Path(str(path) + suffix)
                    if candidate.exists(): candidate.unlink(); removed = True
                return "removed" if removed else "absent"
            if path.is_dir() and not path.is_symlink(): shutil.rmtree(path); return "removed"
            if path.exists() or path.is_symlink(): path.unlink(); return "removed"
            return "absent"
        raise CleanupError("resource requires a provider adapter")

    def _process(self, resource: Any, deadline: float) -> str:
        pid = int(resource.identity)
        try: os.kill(pid, 0)
        except ProcessLookupError: return "absent"
        except PermissionError as error: raise CleanupError("process existence is unknown") from error
        try: observed = isolation._process_start_time(pid)
        except Exception as error: raise CleanupError("process identity is unknown") from error
        nonce_file = Path(str(resource.metadata.get("nonce_file", "")))
        try: nonce = nonce_file.read_text()
        except OSError as error: raise CleanupError("process ownership marker is unreadable") from error
        if observed != str(resource.metadata.get("start_time")) or nonce != resource.metadata.get("nonce"):
            raise CleanupError("process identity changed before TERM")
        group = int(resource.metadata.get("process_group", pid))
        target = -group if os.name != "nt" else pid
        try: os.kill(target, signal.SIGTERM)
        except ProcessLookupError: return "absent"
        grace = min(deadline, time.monotonic() + 1.0)
        while time.monotonic() < grace:
            if not self._process_alive(pid): return "removed"
            time.sleep(0.02)
        try:
            if isolation._process_start_time(pid) != observed or nonce_file.read_text() != nonce:
                raise CleanupError("process identity changed before KILL")
            os.kill(target, signal.SIGKILL)
        except ProcessLookupError: pass
        kill_deadline = min(deadline, time.monotonic() + 1.0)
        while self._process_alive(pid) and time.monotonic() < kill_deadline:
            try: os.waitpid(pid, os.WNOHANG)
            except (AttributeError, ChildProcessError): pass
            time.sleep(0.01)
        try: os.waitpid(pid, os.WNOHANG)
        except (AttributeError, ChildProcessError): pass
        return "force-killed"

    @staticmethod
    def _process_alive(pid: int) -> bool:
        stat_path = Path(f"/proc/{pid}/stat")
        if stat_path.exists():
            try: return stat_path.read_text().split()[2] != "Z"
            except (OSError, IndexError): pass
        try: os.kill(pid, 0); return True
        except ProcessLookupError: return False

    @staticmethod
    def _group_alive(group: int) -> bool:
        if os.name == "nt": return False
        try:
            result = __import__("subprocess").run(
                ["ps", "-eo", "pgid=,state="], capture_output=True, text=True, timeout=1, check=False,
            )
            return any(int(parts[0]) == group and not parts[1].startswith("Z")
                       for line in result.stdout.splitlines() if len(parts := line.split()) >= 2)
        except (OSError, ValueError, __import__("subprocess").TimeoutExpired):
            return True  # uncertainty fails the residual audit closed

    def exists(self, resource: Any) -> bool | None:
        if resource.resource_type == "process":
            try:
                pid = int(resource.identity)
                leader = self._process_alive(pid) and isolation._process_start_time(pid) == str(resource.metadata["start_time"])
                return leader or self._group_alive(int(resource.metadata.get("process_group", pid)))
            except ProcessLookupError: return False
            except Exception as error: raise CleanupError("process residual state is unknown") from error
        if resource.resource_type == "port": return Path(str(resource.metadata.get("lease", ""))).exists()
        if resource.resource_type == "sqlite":
            return any(Path(str(resource.identity) + suffix).exists() for suffix in ("", "-wal", "-shm", "-journal"))
        if resource.resource_type in {"cache", "chrome_diagnostic", "chrome_profile", "credential_file", "data_dir",
                                      "download", "feed_fixture", "git_fixture", "http_cache", "lease", "lock", "output",
                                      "screenshot", "socket", "sqlite_sidecar", "temp_path", "warc"}:
            path = Path(resource.identity); return path.exists() or path.is_symlink()
        return None


@dataclass
class Report:
    run_id: str
    manifest_digest: str
    started_unix_ms: int = field(default_factory=lambda: int(time.time() * 1000))
    created: list[dict[str, str]] = field(default_factory=list)
    removed: list[dict[str, str]] = field(default_factory=list)
    retained: list[dict[str, str]] = field(default_factory=list)
    refused: list[dict[str, str]] = field(default_factory=list)
    residual: list[dict[str, str]] = field(default_factory=list)
    phases: list[dict[str, Any]] = field(default_factory=list)
    classes: dict[str, dict[str, int]] = field(default_factory=dict)
    invariants: list[dict[str, Any]] = field(default_factory=list)

    def item(self, resource: Any, **extra: str) -> dict[str, str]:
        return {"class": resource.resource_type, "opaque_id": resource.opaque_id, **extra}
    def json(self) -> dict[str, Any]:
        return {**self.__dict__, "success": not self.refused and not self.residual,
                "completed_unix_ms": int(time.time() * 1000)}


class Engine:
    def __init__(self, manifest: Path, adapters: dict[str, Adapter] | None = None,
                 *, global_timeout: float = 60, phase_timeout: float = 15, workers: int = 4):
        self.header, self.resources = manifest_api.load(manifest)
        self.local = LocalAdapter(self.header); self.adapters = adapters or {}
        self.global_timeout, self.phase_timeout, self.workers = global_timeout, phase_timeout, workers
        self.report = Report(self.header.run_id, self.header.digest)
        self.report.created = [self.report.item(item) for item in self.resources]

    def adapter(self, resource: Any) -> Adapter:
        if resource.resource_type in {"process", "port", "cache", "chrome_diagnostic", "chrome_profile", "credential_file",
                                      "data_dir", "download", "feed_fixture", "git_fixture", "http_cache", "lease", "lock",
                                      "output", "screenshot", "socket", "sqlite", "sqlite_sidecar", "temp_path", "warc"}: return self.local
        return self.adapters.get(resource.resource_type, RefusingAdapter())

    def provider_lease_state(self) -> dict[str, int]:
        """Read and verify current provider-native ownership lease markers."""
        states: list[dict[str, Any]] = []
        for resource in self.resources:
            if resource.resource_type not in PROVIDER_TYPES: continue
            adapter = self.adapter(resource)
            marker = adapter.marker(resource)
            if marker is None: continue
            manifest_api.verify_marker(self.header, resource, marker)
            states.append(marker)
        if not states: raise CleanupError("no current provider-native lease marker is readable")
        return {
            "heartbeat_unix_ms": max(int(item["heartbeat_unix_ms"]) for item in states),
            "expires_unix_ms": max(int(item["expires_unix_ms"]) for item in states),
        }

    def _one(self, resource: Any, deadline: float) -> None:
        adapter = self.adapter(resource)
        if hasattr(adapter, "set_deadline"): adapter.set_deadline(deadline)
        started = time.monotonic(); before = int(getattr(adapter, "round_trips", 0)); escalated = 0
        try:
            if resource.resource_type == "evidence" and resource.metadata.get("retain") is True:
                if not hasattr(adapter, "sanitize_evidence"):
                    raise CleanupError("evidence retention requires an independent redaction scanner")
                try:
                    proof = adapter.sanitize_evidence(resource)
                    self.report.retained.append(self.report.item(resource, outcome="sanitized-evidence", **proof))
                except Exception as error:
                    outcome = adapter.delete(resource, deadline)
                    self.report.removed.append(self.report.item(resource, outcome="redaction-failed-destroyed",
                                                                reason=str(error), provider_outcome=outcome))
                return
            marker = adapter.marker(resource)
            if resource.resource_type in PROVIDER_TYPES:
                if marker is None and hasattr(adapter, "recover_creating"):
                    outcome = adapter.recover_creating(resource, deadline)
                    exists = adapter.exists(resource)
                    if exists is None or exists: raise CleanupError("setup-intent recovery left provider residue")
                    self.report.removed.append(self.report.item(resource, outcome=f"recovered-{outcome}"))
                    return
                if marker is None: raise CleanupError("provider-native ownership marker is absent")
                manifest_api.verify_marker(self.header, resource, marker)
            outcome = adapter.delete(resource, deadline)
            escalated = int(outcome == "force-killed")
            exists = adapter.exists(resource)
            if exists is None: raise CleanupError("post-delete state is unknown")
            if exists: raise CleanupError("resource remains after cleanup")
            self.report.removed.append(self.report.item(resource, outcome=outcome))
        except Exception as error:
            self.report.refused.append(self.report.item(resource, reason=str(error)))
        finally:
            stats = self.report.classes.setdefault(resource.resource_type,
                {"count": 0, "round_trips": 0, "duration_ms": 0, "escalations": 0,
                 "batch_calls": 0, "unbatchable_calls": 0})
            stats["count"] += 1; stats["round_trips"] += int(getattr(adapter, "round_trips", 0)) - before
            stats["duration_ms"] += int((time.monotonic() - started) * 1000); stats["escalations"] += escalated

    def _batch(self, resources: list[Any], deadline: float) -> None:
        """Verify every exact target, then use the provider's batch operation."""
        if not resources: return
        adapter = self.adapter(resources[0])
        capability = adapter.batch_capability(resources[0].resource_type) if hasattr(adapter, "batch_capability") \
            else "unbatchable-adapter"
        if not hasattr(adapter, "delete_batch") or len(resources) == 1:
            for resource in resources: self._one(resource, deadline)
            return
        started = time.monotonic(); before = int(getattr(adapter, "round_trips", 0))
        eligible: list[Any] = []
        for resource in resources:
            try:
                if resource.resource_type == "evidence" and resource.metadata.get("retain") is True:
                    if not hasattr(adapter, "sanitize_evidence"):
                        raise CleanupError("evidence retention requires an independent redaction scanner")
                    try:
                        proof = adapter.sanitize_evidence(resource)
                        self.report.retained.append(self.report.item(resource, outcome="sanitized-evidence", **proof))
                    except Exception as error:
                        outcome = adapter.delete(resource, deadline)
                        self.report.removed.append(self.report.item(resource, outcome="redaction-failed-destroyed",
                                                                    reason=str(error), provider_outcome=outcome))
                    continue
                marker = adapter.marker(resource)
                if resource.resource_type in PROVIDER_TYPES:
                    if marker is None and hasattr(adapter, "recover_creating"):
                        outcome = adapter.recover_creating(resource, deadline)
                        state = adapter.exists(resource)
                        if state is None or state: raise CleanupError("setup-intent recovery left provider residue")
                        self.report.removed.append(self.report.item(resource, outcome=f"recovered-{outcome}"))
                        continue
                    if marker is None: raise CleanupError("provider-native ownership marker is absent")
                    manifest_api.verify_marker(self.header, resource, marker)
                eligible.append(resource)
            except Exception as error: self.report.refused.append(self.report.item(resource, reason=str(error)))
        try:
            outcomes = adapter.delete_batch(eligible, deadline)
            if len(outcomes) != len(eligible): raise CleanupError("batch adapter returned an incomplete result")
            for resource, outcome in outcomes:
                state = adapter.exists(resource)
                if state is None: raise CleanupError("post-delete state is unknown")
                if state: raise CleanupError("resource remains after cleanup")
                self.report.removed.append(self.report.item(resource, outcome=outcome))
        except Exception as error:
            completed = {(item["class"], item["opaque_id"]) for item in self.report.removed}
            for resource in eligible:
                if (resource.resource_type, resource.opaque_id) not in completed:
                    self.report.refused.append(self.report.item(resource, reason=str(error)))
        finally:
            elapsed = int((time.monotonic() - started) * 1000)
            delta = int(getattr(adapter, "round_trips", 0)) - before
            for resource in resources:
                stats = self.report.classes.setdefault(resource.resource_type,
                    {"count": 0, "round_trips": 0, "duration_ms": 0, "escalations": 0,
                     "batch_calls": 0, "unbatchable_calls": 0})
                stats["count"] += 1; stats["duration_ms"] += elapsed
            if resources:
                stats = self.report.classes[resources[0].resource_type]
                stats["round_trips"] += delta
                if capability.startswith("unbatchable"): stats["unbatchable_calls"] += 1
                else: stats["batch_calls"] += 1

    def run(self) -> Report:
        global_deadline = time.monotonic() + self.global_timeout
        owned = {(item.resource_type, item.identity) for item in self.resources}
        snapshots: dict[int, tuple[Any, Any]] = {}
        for item in self.resources:
            adapter = self.adapter(item); key = id(adapter)
            if key in snapshots or not hasattr(adapter, "snapshot_shared"): continue
            try:
                if hasattr(adapter, "discover_unregistered"):
                    for orphan in adapter.discover_unregistered(self.header.run_id, owned):
                        opaque = hashlib.sha256(f"{orphan['resource_type']}\0{orphan['identity']}".encode()).hexdigest()[:20]
                        self.report.refused.append({"class": orphan["resource_type"], "opaque_id": opaque,
                                                    "reason": "provider residue exists before manifest/provider-ledger persistence"})
                snapshots[key] = (adapter, adapter.snapshot_shared(owned))
            except Exception as error:
                self.report.residual.append({"class": "shared_state", "opaque_id": f"adapter-{key:x}",
                                             "reason": f"pre-cleanup snapshot unknown: {error}"})
        handled: set[int] = set()
        for phase_name, types in PHASES:
            started = time.monotonic(); deadline = min(global_deadline, started + self.phase_timeout)
            batch = [item for item in reversed(self.resources) if item.resource_type in types]
            handled.update(item.sequence for item in batch)
            adapters = {id(self.adapter(item)): self.adapter(item) for item in batch}
            before = sum(int(getattr(adapter, "round_trips", 0)) for adapter in adapters.values())
            escalations_before = sum(item.get("outcome") == "force-killed" for item in self.report.removed)
            groups: dict[tuple[int, str], list[Any]] = {}
            for item in batch:
                groups.setdefault((id(self.adapter(item)), item.resource_type), []).append(item)
            for group in groups.values():
                if time.monotonic() >= deadline:
                    for item in group: self.report.refused.append(self.report.item(item, reason="phase deadline exceeded"))
                    continue
                self._batch(group, deadline)
            after = sum(int(getattr(adapter, "round_trips", 0)) for adapter in adapters.values())
            self.report.phases.append({"name": phase_name, "count": len(batch),
                                       "duration_ms": int((time.monotonic() - started) * 1000),
                                       "round_trips": after - before,
                                       "escalations": sum(item.get("outcome") == "force-killed" for item in self.report.removed)
                                                      - escalations_before})
            if time.monotonic() >= global_deadline: break
        for item in self.resources:
            if item.sequence not in handled:
                self.report.refused.append(self.report.item(item, reason="unsupported resource class"))
        self.audit()
        for key, (adapter, before_state) in snapshots.items():
            try:
                after_state = adapter.snapshot_shared(owned); unchanged = before_state == after_state
                self.report.invariants.append({"adapter": type(adapter).__name__, "unchanged": unchanged,
                                               "before_sha256": canonical_state_digest(before_state),
                                               "after_sha256": canonical_state_digest(after_state)})
                if not unchanged:
                    self.report.residual.append({"class": "shared_state", "opaque_id": f"adapter-{key:x}",
                                                 "reason": "unowned provider/operator state changed"})
            except Exception as error:
                self.report.residual.append({"class": "shared_state", "opaque_id": f"adapter-{key:x}",
                                             "reason": f"post-cleanup snapshot unknown: {error}"})
        return self.report

    def audit(self) -> None:
        removed_ids = {item["opaque_id"] for item in self.report.removed}
        for item in self.resources:
            # Every successful deletion is audited synchronously before its
            # backing SQLite/data directory can be removed by later phases.
            if item.opaque_id in removed_ids: continue
            if item.resource_type == "evidence" and item.metadata.get("retain") is True \
                    and any(retained["opaque_id"] == item.opaque_id for retained in self.report.retained):
                continue
            try: state = self.adapter(item).exists(item)
            except Exception as error:
                self.report.residual.append(self.report.item(item, reason=f"audit state unknown: {error}")); continue
            if state is None:
                self.report.residual.append(self.report.item(item, reason="audit state unknown"))
            elif state:
                self.report.residual.append(self.report.item(item, reason="exact identity still exists"))


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path)
    parser.add_argument("--report", type=Path, required=True); parser.add_argument("--global-timeout", type=float, default=60)
    parser.add_argument("--provider-config", type=Path)
    args = parser.parse_args()
    try:
        header, _ = manifest_api.load(args.manifest)
        adapters = provider_api.build(args.provider_config, header, manifest_api) if args.provider_config else None
        report = Engine(args.manifest, adapters, global_timeout=args.global_timeout).run().json()
    except Exception as error: report = {"success": False, "fatal": str(error)}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report.get("success") else 2


if __name__ == "__main__": raise SystemExit(main())
