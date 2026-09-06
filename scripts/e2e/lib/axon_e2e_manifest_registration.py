"""Validation for append-only E2E manifest resource registration."""

from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any, Callable


def register_resource(
    manifest: Any,
    resource_type: str,
    identity: str,
    metadata: dict[str, Any] | None,
    *,
    resource_types: set[str],
    run_prefix: str,
    provider_ceilings: dict[str, int],
    validate_owned_name: Callable[[str], None],
    canonical: Callable[[Path], Path],
    is_within: Callable[[Path, Path], bool],
    process_start_time: Callable[[int], str],
    error_type: type[RuntimeError],
) -> None:
    """Validate and append one exact resource without weakening ownership."""
    if resource_type not in resource_types:
        raise error_type(f"unsupported resource type: {resource_type}")
    metadata = metadata or {}
    records = manifest.verify()
    header = records[0]["payload"]
    namespaced_types = {"chat_session", "collection", "compose_project", "container", "evidence", "network",
                        "operation", "qdrant_alias", "qdrant_snapshot", "tailscale_node", "volume"}
    if resource_type in namespaced_types:
        validate_owned_name(identity)
        if identity != header["run_id"] and not identity.startswith(header["run_id"] + "_"):
            raise error_type(f"{resource_type} identity belongs to a different local/CI/rerun namespace")
    if resource_type in {"collection", "qdrant_alias", "qdrant_snapshot", "point", "payload_index"}:
        generation = metadata.get("ownership_generation")
        if not isinstance(generation, str) or len(generation) < 32:
            raise error_type(f"{resource_type} requires a strong ownership_generation before provider setup")
        if resource_type != "collection" and (not isinstance(metadata.get("collection"), str)
                                               or not metadata["collection"].startswith(run_prefix)):
            raise error_type(f"{resource_type} requires an owned collection binding")
    if resource_type in {"artifact", "upload", "watch"} and not identity.startswith(run_prefix):
        prefix = {"artifact": "art_", "upload": "upl_", "watch": "watch_"}[resource_type]
        required = {"run_id", "attempt", "scenario_id", "request_id", "origin",
                    "parent_resource_type", "parent_identity"}
        if not re.fullmatch(rf"{prefix}[A-Za-z0-9_-]{{3,128}}", identity):
            raise error_type(f"opaque {resource_type} identity has an invalid production format")
        if not required.issubset(metadata) or metadata.get("origin") != "server_response":
            raise error_type(f"opaque {resource_type} registration lacks trusted server binding")
        if metadata.get("run_id") != header["run_id"] or not isinstance(metadata.get("attempt"), int) or metadata["attempt"] < 1:
            raise error_type(f"opaque {resource_type} registration is not bound to this run attempt")
        parent_type, parent_identity = metadata["parent_resource_type"], metadata["parent_identity"]
        if parent_type not in {"operation", "evidence"}:
            raise error_type(f"opaque {resource_type} parent must be an operation or evidence record")
        parent_found = any(record.get("payload", {}).get("kind") == "resource"
                           and record["payload"].get("resource_type") == parent_type
                           and record["payload"].get("identity") == parent_identity
                           for record in records)
        if not parent_found:
            raise error_type(f"opaque {resource_type} parent is not registered in this manifest")
    elif resource_type in {"artifact", "upload", "watch"}:
        validate_owned_name(identity)
    data_dir = Path(header["data_dir"])
    if resource_type in {"job", "source"}:
        if not identity or any(char in identity for char in "\r\n\t"):
            raise error_type(f"{resource_type} identity must be non-empty and single-line")
        if metadata.get("run_id") != header["run_id"]:
            raise error_type(f"{resource_type} registration must be bound to the manifest run")
    if resource_type == "chat_session":
        if metadata.get("run_id") != header["run_id"] or not isinstance(metadata.get("scenario_id"), str) \
                or not metadata["scenario_id"].strip():
            raise error_type("chat_session registration requires this run_id and a scenario_id")
    if resource_type == "provider_reservation":
        if metadata.get("run_id") != header["run_id"]:
            raise error_type("provider_reservation registration must be bound to this run")
        if metadata.get("provider") not in provider_ceilings:
            raise error_type("provider_reservation provider is not recognized")
        for field in ("permits", "requests", "retries"):
            value = metadata.get(field, 0)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise error_type(f"provider_reservation {field} must be a nonnegative integer")
    if resource_type == "data_dir" and canonical(Path(identity)) != canonical(data_dir):
        raise error_type("data_dir identity does not match the owned manifest header")
    if resource_type == "sqlite" and not is_within(Path(identity), data_dir):
        raise error_type("SQLite identity must remain inside the owned AXON_DATA_DIR")
    path_types = {"cache", "chrome_diagnostic", "chrome_profile", "credential_file", "download",
                  "feed_fixture", "git_fixture", "http_cache", "lease", "lock", "output", "screenshot",
                  "socket", "sqlite_sidecar", "temp_path", "warc"}
    if resource_type in path_types and not is_within(Path(identity), data_dir.parent):
        raise error_type(f"{resource_type} path must remain inside the owned run root")
    if resource_type == "port":
        try:
            port = int(identity)
        except ValueError as error:
            raise error_type("port identity must be an integer") from error
        if not 1 <= port <= 65535 or metadata.get("host") not in {"127.0.0.1", "::1"}:
            raise error_type("port identity must be an owned loopback endpoint")
        lease = Path(str(metadata.get("lease", "")))
        try:
            lease_owner = json.loads((lease / "owner.json").read_text(encoding="utf-8"))["run_id"]
        except (OSError, KeyError, json.JSONDecodeError) as error:
            raise error_type("port registration requires a readable cooperative lease") from error
        if lease_owner != header["run_id"]:
            raise error_type("port lease is not owned by this manifest run")
    if resource_type == "process":
        try:
            pid = int(identity)
        except ValueError as error:
            raise error_type("process identity must be a PID") from error
        required = {"start_time", "nonce", "nonce_file"}
        if pid < 1 or not required.issubset(metadata) or len(str(metadata["nonce"])) < 32:
            raise error_type("process registration requires PID, start time, and strong nonce ownership")
        nonce_file = Path(str(metadata["nonce_file"]))
        try:
            nonce_matches = nonce_file.read_text(encoding="utf-8") == metadata["nonce"]
        except OSError as error:
            raise error_type("process nonce ownership marker is unreadable") from error
        if not is_within(nonce_file, data_dir.parent) or not nonce_matches:
            raise error_type("process nonce marker is not owned by this run")
        if process_start_time(pid) != str(metadata["start_time"]):
            raise error_type("process PID start time does not match the live process")
    manifest._append({
        "kind": "resource", "resource_type": resource_type, "identity": identity,
        "metadata": metadata, "registered_unix_ms": int(time.time() * 1000),
    })
