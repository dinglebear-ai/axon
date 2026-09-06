#!/usr/bin/env python3
"""Validate or mechanically refresh the canonical Axon E2E corpus manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any

CORPUS_DIR = Path(__file__).resolve().parent
MANIFEST_PATH = CORPUS_DIR / "manifest.json"
BASELINE_PATH = CORPUS_DIR / "release-baseline.json"
LICENSE_ALLOWLIST_PATH = CORPUS_DIR / "license-allowlist.json"
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
SENSITIVE_PATTERNS = (
    re.compile(rb"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"),
    re.compile(rb"AKIA[0-9A-Z]{16}"),
    re.compile(rb"ASIA[0-9A-Z]{16}"),
    re.compile(rb"gh[opsu]_[A-Za-z0-9]{30,}"),
    re.compile(rb"sk-[A-Za-z0-9]{32,}"),
    re.compile(rb"AIza[0-9A-Za-z_-]{35}"),
    re.compile(rb"xox[baprs]-[0-9A-Za-z-]{20,}"),
    re.compile(rb"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"),
    re.compile(rb"(?i)authorization\s*:\s*bearer\s+[A-Za-z0-9._~+/=-]{16,}"),
    re.compile(rb"(?i)(?:api[_-]?key|client[_-]?secret|password)\s*[:=]\s*['\"]?[A-Za-z0-9._~+/=-]{16,}"),
)


class CorpusError(ValueError):
    """A deterministic corpus contract violation."""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest_records(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    records = [*manifest["documents"], *manifest["expectations"], *manifest["revisions"]]
    records.append(manifest["stress_recipe"])
    return records


def corpus_checksum(manifest: dict[str, Any]) -> str:
    canonical = dict(manifest)
    canonical["corpus_checksum"] = ""
    encoded = json.dumps(canonical, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def component_content_checksums(manifest: dict[str, Any]) -> dict[str, str]:
    grouped: dict[str, list[tuple[str, str]]] = {
        "bytes": [], "semantics": [], "chunking": [], "retrieval": []
    }
    for record in manifest_records(manifest):
        grouped[record.get("kind", "bytes")].append((record["path"], record["sha256"]))
    return {
        component: hashlib.sha256(
            json.dumps(sorted(records), separators=(",", ":")).encode()
        ).hexdigest()
        for component, records in grouped.items()
    }


def stress_record(recipe: dict[str, Any], index: int) -> str:
    if not 0 <= index < recipe["document_count"]:
        raise IndexError(index)
    token = hashlib.sha256(f'{recipe["seed"]}:{index}'.encode()).hexdigest()[
        : recipe["token_hex_chars"]
    ]
    return recipe["template"].format(
        index=index, token=token, group=index % recipe["group_modulus"]
    )


def rewrite_checksums(path: Path = MANIFEST_PATH) -> dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    root = path.parent
    baseline_path = root / BASELINE_PATH.name
    baseline = json.loads(baseline_path.read_text(encoding="utf-8")) if baseline_path.exists() else None
    changed_components: set[str] = set()
    for record in manifest_records(manifest):
        actual_hash = sha256(root / record["path"])
        if record.get("sha256") != actual_hash:
            changed_components.add(record.get("kind", "bytes"))
        record["sha256"] = actual_hash
    if baseline:
        current_content = component_content_checksums(manifest)
        baseline_content = baseline.get("component_content_checksums")
        if baseline_content:
            changed_components = {
                component for component, digest in current_content.items()
                if baseline_content.get(component) != digest
            }
        unchanged_versions = [
            component
            for component in sorted(changed_components)
            if manifest["components"].get(component) == baseline["components"].get(component)
        ]
        if unchanged_versions:
            raise CorpusError(
                "changed corpus content requires component version bump: "
                + ", ".join(unchanged_versions)
            )
    manifest["corpus_checksum"] = corpus_checksum(manifest)
    path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return manifest


def validate(path: Path = MANIFEST_PATH) -> dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    root = path.parent
    errors: list[str] = []

    for field in ("corpus_version",):
        if not SEMVER.fullmatch(str(manifest.get(field, ""))):
            errors.append(f"{field} must be semantic version")
    components = manifest.get("components", {})
    if set(components) != {"bytes", "semantics", "chunking", "retrieval"}:
        errors.append("components must independently version bytes, semantics, chunking, retrieval")
    for name, version in components.items():
        if not SEMVER.fullmatch(str(version)):
            errors.append(f"component {name} must be semantic version")

    allowlist = json.loads((root / LICENSE_ALLOWLIST_PATH.name).read_text(encoding="utf-8"))
    license_spdx = manifest.get("license_spdx")
    if license_spdx not in allowlist.get("allowed_spdx", []):
        errors.append(f"license SPDX identifier is not allowlisted: {license_spdx}")

    records = manifest_records(manifest)
    paths = [record.get("path") for record in records]
    if len(paths) != len(set(paths)):
        errors.append("manifest paths must be unique")
    ids = [record["id"] for section in ("documents", "revisions") for record in manifest[section]]
    if len(ids) != len(set(ids)):
        errors.append("document and revision IDs must be unique")

    declared = set(paths)
    version_root = root / manifest.get("root", "missing")
    actual = {
        file.relative_to(root).as_posix()
        for file in version_root.rglob("*")
        if file.is_file()
    } if version_root.is_dir() else set()
    if undeclared := sorted(actual - declared):
        errors.append(f"undeclared corpus files: {undeclared}")
    if missing := sorted(declared - actual):
        errors.append(f"missing corpus files: {missing}")

    for record in records:
        fixture = root / record["path"]
        if fixture.is_file():
            actual_hash = sha256(fixture)
            if record.get("sha256") != actual_hash:
                errors.append(f'checksum mismatch: {record["path"]}')
            data = fixture.read_bytes()
            for pattern in SENSITIVE_PATTERNS:
                if pattern.search(data):
                    errors.append(f'credential-like data in {record["path"]}')
            lowered = data.lower()
            for marker in allowlist.get("prohibited_markers", []):
                if marker.encode().lower() in lowered:
                    errors.append(f'prohibited license marker in {record["path"]}: {marker}')

    oversized = [d for d in manifest["documents"] if d.get("expected_parse") == "reject_oversized"]
    if not oversized:
        errors.append("an explicit oversized rejection fixture is required")
    for document in oversized:
        fixture = root / document["path"]
        if fixture.is_file():
            size = fixture.stat().st_size
            if size < document.get("minimum_fixture_bytes", 0):
                errors.append(f'oversized fixture is too small: {document["path"]}')
            if size <= document.get("declared_input_limit_bytes", size):
                errors.append(f'oversized fixture does not exceed declared limit: {document["path"]}')

    revisions = {item["id"]: item for item in manifest["revisions"]}
    for revision in revisions.values():
        predecessor = revision.get("predecessor")
        if predecessor is not None:
            prior = revisions.get(predecessor)
            if prior is None:
                errors.append(f'broken lineage: {revision["id"]} -> {predecessor}')
                continue
            if prior["source_id"] != revision["source_id"]:
                errors.append(f'source identity changed in lineage: {revision["id"]}')
            hashes_equal = prior.get("sha256") == revision.get("sha256")
            if revision["change"] == "unchanged" and not hashes_equal:
                errors.append(f'unchanged revision bytes differ: {revision["id"]}')
            if revision["change"] == "changed" and hashes_equal:
                errors.append(f'changed revision bytes are identical: {revision["id"]}')

    documents = manifest["documents"]
    for tier in ("micro", "representative"):
        included = [d for d in documents if d["tier"] == tier]
        if tier == "representative":
            included += [d for d in documents if d["tier"] == "micro"]
        limits = manifest["tiers"][tier]
        byte_count = sum((root / d["path"]).stat().st_size for d in included if (root / d["path"]).is_file())
        if len(included) > limits["max_documents"]:
            errors.append(f"{tier} document bound exceeded")
        if byte_count > limits["max_bytes"]:
            errors.append(f"{tier} byte bound exceeded")
    if manifest["tiers"]["stress"]["selection"] != "explicit-capacity-only":
        errors.append("stress tier must remain explicit-capacity-only")

    expected_checksum = corpus_checksum(manifest)
    if manifest.get("corpus_checksum") != expected_checksum:
        errors.append("aggregate corpus checksum mismatch")
    if errors:
        raise CorpusError("; ".join(errors))
    return {
        "corpus_version": manifest["corpus_version"],
        "corpus_checksum": manifest["corpus_checksum"],
        "documents": len(documents),
        "status": "valid",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=MANIFEST_PATH)
    parser.add_argument("--rewrite-checksums", action="store_true")
    parser.add_argument("--accept-release", action="store_true")
    args = parser.parse_args()
    if args.rewrite_checksums:
        rewrite_checksums(args.manifest)
    report = validate(args.manifest)
    if args.accept_release:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        baseline = {
            "schema_version": 1,
            "corpus_version": manifest["corpus_version"],
            "corpus_checksum": manifest["corpus_checksum"],
            "components": manifest["components"],
            "component_content_checksums": component_content_checksums(manifest),
        }
        (args.manifest.parent / BASELINE_PATH.name).write_text(
            json.dumps(baseline, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
