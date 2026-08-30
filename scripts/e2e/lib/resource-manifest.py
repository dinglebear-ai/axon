#!/usr/bin/env python3
"""Read-only cleanup view over the authoritative run-isolation manifest."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import importlib.util
import json
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any


def _load_isolation():
    path = Path(__file__).with_name("run-isolation.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_run_isolation", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("run-isolation module is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


isolation = _load_isolation()


class ManifestError(RuntimeError):
    pass


@dataclass(frozen=True)
class RunHeader:
    run_id: str
    data_dir: Path
    created_unix_ms: int
    digest: str
    manifest_path: Path


@dataclass(frozen=True)
class Resource:
    resource_type: str
    identity: str
    metadata: dict[str, Any]
    sequence: int
    checkpoint_digest: str

    @property
    def opaque_id(self) -> str:
        value = f"{self.resource_type}\0{self.identity}".encode()
        return hashlib.sha256(value).hexdigest()[:20]


def canonical_digest(records: list[dict[str, Any]]) -> str:
    encoded = json.dumps(records, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def load(path: Path) -> tuple[RunHeader, list[Resource]]:
    try:
        records = isolation.Manifest.open(path).verify()
    except Exception as error:
        raise ManifestError(f"manifest verification failed: {error}") from error
    payload = records[0]["payload"]
    header = RunHeader(
        run_id=payload["run_id"], data_dir=Path(payload["data_dir"]).resolve(),
        created_unix_ms=int(payload["created_unix_ms"]), digest=canonical_digest(records), manifest_path=path.resolve(),
    )
    resources = []
    seen: set[tuple[str, str]] = set()
    for record in records[1:]:
        item = record["payload"]
        if item.get("kind") != "resource":
            continue
        key = (str(item["resource_type"]), str(item["identity"]))
        if key in seen:
            continue
        seen.add(key)
        resources.append(Resource(*key, dict(item.get("metadata", {})), int(record["sequence"]), str(record["hmac"])))
    return header, resources


def provider_marker(header: RunHeader, resource: Resource, *, attempt: int = 1,
                    owner: str = "axon-e2e") -> dict[str, Any]:
    return {
        "schema": 1, "run_id": header.run_id, "attempt": attempt, "owner": owner,
        # The record-chain checkpoint is stable even as later resources append.
        "manifest_digest": resource.checkpoint_digest, "resource_type": resource.resource_type,
        "identity": resource.identity, "created_unix_ms": header.created_unix_ms,
        "targets": [{"resource_type": resource.resource_type, "identity": resource.identity}],
        "heartbeat_unix_ms": int(resource.metadata.get("heartbeat_unix_ms", header.created_unix_ms)),
        "expires_unix_ms": int(resource.metadata.get("expires_unix_ms", header.created_unix_ms + 21_600_000)),
        "repository": resource.metadata.get("repository", "dinglebear-ai/axon"),
        "workflow": resource.metadata.get("workflow", "local"),
    }


def qdrant_ownership_point(header: RunHeader, resource: Resource) -> dict[str, Any]:
    """Return the durable marker point setup must upsert with the resource."""
    generation = str(resource.metadata.get("ownership_generation", ""))
    if not generation:
        raise ManifestError("Qdrant ownership requires an unpredictable ownership_generation")
    point_id = str(uuid.UUID(bytes=hashlib.sha256(
        f"{header.run_id}\0{resource.resource_type}\0{resource.identity}\0{generation}".encode()).digest()[:16]))
    configured = resource.metadata.get("ownership_point_id")
    if configured is not None and configured != point_id:
        raise ManifestError("Qdrant ownership point id does not match its generation binding")
    return {"id": point_id, "vector": {}, "payload": {
        "axon_e2e_ownership": {**provider_marker(header, resource), "generation": generation},
        "axon_e2e_marker": True,
        "payload_contract_version": "2026-07-01",
    }}


def _ledger_path(header: RunHeader) -> Path:
    return header.manifest_path.with_suffix(header.manifest_path.suffix + ".provider-ledger")


def _ledger_key(header: RunHeader) -> bytes:
    return isolation.Manifest.open(header.manifest_path)._key()


def write_provider_ledger(header: RunHeader, resource: Resource, provider_state: Any) -> dict[str, Any]:
    """Persist a signed provider fingerprint outside the ephemeral run root."""
    path = _ledger_path(header); entries = read_provider_ledger(header, allow_missing=True)
    generation = str(resource.metadata.get("ownership_generation", ""))
    if not generation: raise ManifestError("external provider ledger requires ownership_generation")
    state_digest = hashlib.sha256(json.dumps(provider_state, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    entry = {"resource_type": resource.resource_type, "identity": resource.identity, "generation": generation, "status": "owned",
             "checkpoint_digest": resource.checkpoint_digest, "provider_state_sha256": state_digest,
             "marker": provider_marker(header, resource)}
    entries = [item for item in entries if (item["resource_type"], item["identity"]) != (resource.resource_type, resource.identity)] + [entry]
    payload = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    envelope = {"entries": entries, "hmac": hmac.new(_ledger_key(header), payload, hashlib.sha256).hexdigest()}
    temporary = path.with_suffix(path.suffix + ".tmp"); temporary.write_text(json.dumps(envelope, sort_keys=True) + "\n")
    temporary.chmod(0o600); temporary.replace(path); return entry


def write_setup_intent(header: RunHeader, resource: Resource) -> dict[str, Any]:
    """Durably record the exact generation before the provider create call."""
    path = _ledger_path(header); entries = read_provider_ledger(header, allow_missing=True)
    generation = str(resource.metadata.get("ownership_generation", ""))
    if len(generation) < 32: raise ManifestError("setup intent requires a strong ownership_generation")
    entry = {"resource_type": resource.resource_type, "identity": resource.identity,
             "generation": generation, "checkpoint_digest": resource.checkpoint_digest,
             "status": "creating", "marker": provider_marker(header, resource)}
    entries = [item for item in entries if (item.get("resource_type"), item.get("identity")) !=
               (resource.resource_type, resource.identity)] + [entry]
    payload = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    envelope = {"entries": entries, "hmac": hmac.new(_ledger_key(header), payload, hashlib.sha256).hexdigest()}
    temporary = path.with_suffix(path.suffix + ".tmp"); temporary.write_text(json.dumps(envelope, sort_keys=True) + "\n")
    temporary.chmod(0o600); temporary.replace(path); return entry


def verify_setup_intent(header: RunHeader, resource: Resource) -> dict[str, Any]:
    matches = [item for item in read_provider_ledger(header)
               if (item.get("resource_type"), item.get("identity")) == (resource.resource_type, resource.identity)]
    if len(matches) != 1 or matches[0].get("status") != "creating":
        raise ManifestError("signed provider setup intent is absent")
    entry = matches[0]
    if entry.get("generation") != resource.metadata.get("ownership_generation") or \
            entry.get("checkpoint_digest") != resource.checkpoint_digest:
        raise ManifestError("provider setup intent generation/checkpoint changed")
    verify_marker(header, resource, entry["marker"]); return entry


def read_provider_ledger(header: RunHeader, *, allow_missing: bool = False) -> list[dict[str, Any]]:
    path = _ledger_path(header)
    if not path.exists():
        if allow_missing: return []
        raise ManifestError("durable provider ownership ledger is absent")
    envelope = json.loads(path.read_text()); entries = envelope.get("entries")
    if not isinstance(entries, list): raise ManifestError("durable provider ledger is malformed")
    payload = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    expected = hmac.new(_ledger_key(header), payload, hashlib.sha256).hexdigest()
    if not hmac.compare_digest(expected, str(envelope.get("hmac", ""))): raise ManifestError("durable provider ledger integrity failure")
    return entries


def verify_provider_ledger(header: RunHeader, resource: Resource, provider_state: Any) -> dict[str, Any]:
    matches = [item for item in read_provider_ledger(header)
               if (item.get("resource_type"), item.get("identity")) == (resource.resource_type, resource.identity)]
    if len(matches) != 1: raise ManifestError("durable provider ledger identity is missing or ambiguous")
    entry = matches[0]; generation = resource.metadata.get("ownership_generation")
    if entry.get("status") != "owned": raise ManifestError("provider setup did not reach owned state")
    if entry.get("generation") != generation or entry.get("checkpoint_digest") != resource.checkpoint_digest:
        raise ManifestError("durable provider ledger generation/checkpoint changed")
    digest = hashlib.sha256(json.dumps(provider_state, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if entry.get("provider_state_sha256") != digest: raise ManifestError("provider identity was recycled or mutated")
    verify_marker(header, resource, entry["marker"]); return entry["marker"]


def verify_marker(header: RunHeader, resource: Resource, marker: dict[str, Any]) -> None:
    expected = provider_marker(
        header, resource, attempt=int(resource.metadata.get("attempt", 1)),
        owner=str(resource.metadata.get("owner", "axon-e2e")),
    )
    for field in ("schema", "run_id", "attempt", "owner", "manifest_digest",
                  "resource_type", "identity", "repository", "workflow",
                  "heartbeat_unix_ms", "expires_unix_ms"):
        if marker.get(field) != expected[field]:
            raise ManifestError(f"provider marker mismatch for {resource.opaque_id}: {field}")
    if marker.get("targets") != expected["targets"]:
        raise ManifestError(f"provider marker mismatch for {resource.opaque_id}: targets")


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path)
    parser.add_argument("--provider-markers", action="store_true"); args = parser.parse_args()
    try:
        header, resources = load(args.manifest)
        output = {
            "run_id": header.run_id, "manifest_digest": header.digest,
            "resources": [
                ({"class": item.resource_type, "opaque_id": item.opaque_id,
                  "marker": provider_marker(header, item)} if args.provider_markers else
                 {"class": item.resource_type, "opaque_id": item.opaque_id})
                for item in resources
            ],
        }
        print(json.dumps(output, sort_keys=True)); return 0
    except ManifestError as error:
        print(f"manifest error: {error}", file=sys.stderr); return 2


if __name__ == "__main__": raise SystemExit(main())
