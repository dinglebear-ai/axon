#!/usr/bin/env python3
"""Assemble a checksum-bound qualification bundle from verified lane outputs."""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import importlib.util
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError(f"JSON object required: {path}")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    spec = load(args.spec)
    tested_sha = spec.get("tested_sha")
    if not isinstance(tested_sha, str) or len(tested_sha) != 40:
        raise ValueError("full tested SHA required")
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    catalog_source = ROOT / "tests/e2e/catalog/catalog.json"
    corpus_source = ROOT / "tests/e2e/corpus/manifest.json"
    shutil.copyfile(catalog_source, output / "catalog.json")
    shutil.copyfile(corpus_source, output / "corpus.json")
    catalog = load(output / "catalog.json")
    corpus = load(output / "corpus.json")
    artifacts = []
    producer_family_runs: set[tuple[str, str]] = set()
    for item in spec.get("artifacts", []):
        source = Path(item["source"]).resolve(strict=True)
        value = load(source)
        if value.get("tested_sha") != tested_sha:
            raise ValueError(f"artifact tested SHA mismatch: {item['id']}")
        destination = output / "artifacts" / f"{item['id']}.json"
        destination.parent.mkdir(parents=True, exist_ok=True)
        evidence_format = item["format"]
        scenario_id = item.get("canonical_scenario")
        if scenario_id is not None:
            module_path = ROOT / "scripts/e2e/lib/reporting.py"
            module_spec = importlib.util.spec_from_file_location("qualification_assembly_reporting", module_path)
            if module_spec is None or module_spec.loader is None: raise ValueError("report validator unavailable")
            reporting = importlib.util.module_from_spec(module_spec);sys.modules[module_spec.name]=reporting;module_spec.loader.exec_module(reporting)
            reporting.validate_report(value)
            selected = value["scenarios"] if scenario_id == "*" else [row for row in value["scenarios"] if row["scenario_id"] == scenario_id]
            if not selected: raise ValueError(f"canonical scenario missing: {scenario_id}")
            passed = all(row["status"] == "passed" for row in selected)
            projection = {"source_report_sha256": value["report_sha256"],
                          "scenario_ids": sorted(row["scenario_id"] for row in selected),
                          "statuses": sorted({row["status"] for row in selected})}
            projected = {"tested_sha": tested_sha, "release_qualification": {
                "result": "pass" if passed else "fail", "projection": projection}}
            destination.write_text(json.dumps(projected, indent=2, sort_keys=True) + "\n")
            evidence_format = "qualification-record"
        else:
            shutil.copyfile(source, destination)
        data = destination.read_bytes()
        producer = item.get("producer", {})
        if producer.get("tested_sha") != tested_sha:
            raise ValueError(f"producer tested SHA mismatch: {item['id']}")
        producer_family_run = (item["family"], str(producer.get("run_id")))
        if producer_family_run in producer_family_runs:
            raise ValueError(f"duplicate producer run for family: {item['family']}")
        producer_family_runs.add(producer_family_run)
        artifacts.append({
            "id": item["id"], "family": item["family"],
            "path": destination.relative_to(output).as_posix(),
            "sha256": sha(data), "bytes": len(data),
            "redaction_class": item.get("redaction_class", "sanitized"),
            "format": evidence_format,
            "retention": {"location": "github-artifact", "days": 30},
            "producer": producer,
        })
    catalog_bytes = (output / "catalog.json").read_bytes()
    corpus_bytes = (output / "corpus.json").read_bytes()
    index = {
        "schema": 1, "profile": spec["profile"],
        "policy_version": spec["policy_version"], "tested_sha": tested_sha,
        "as_of": spec["as_of"], "not_applicable": spec.get("not_applicable", {}),
        "subject": {
            "tested_sha": tested_sha, "product_version": spec["product_version"],
            "catalog_version": catalog["schema_version"], "catalog_sha256": sha(catalog_bytes),
            "corpus_version": corpus["corpus_version"], "corpus_sha256": sha(corpus_bytes),
            "sources": {
                "catalog": {"path": "catalog.json", "sha256": sha(catalog_bytes)},
                "corpus": {"path": "corpus.json", "sha256": sha(corpus_bytes)},
            },
        },
        "artifacts": artifacts,
    }
    assembly = spec.get("assembly")
    if (not isinstance(assembly, dict) or assembly.get("tested_sha") != tested_sha
            or assembly.get("workflow") != ".github/workflows/e2e-qualification-assemble.yml"):
        raise ValueError("assembly provenance is missing or SHA-mismatched")
    attestation = {"schema": 1, "tested_sha": tested_sha, "assembly": assembly,
                   "artifacts": [{"id": item["id"], "family": item["family"],
                                  "sha256": item["sha256"], "producer": item["producer"]}
                                 for item in artifacts]}
    attestation["attestation_sha256"] = sha(json.dumps(attestation, sort_keys=True, separators=(",", ":")).encode())
    attestation_bytes = (json.dumps(attestation, indent=2, sort_keys=True) + "\n").encode()
    (output / "assembly-attestation.json").write_bytes(attestation_bytes)
    index["assembly_attestation"] = {"path": "assembly-attestation.json", "sha256": sha(attestation_bytes),
                                     "workflow": assembly.get("workflow"), "run_id": assembly.get("run_id")}
    (output / "qualification-index.json").write_text(json.dumps(index, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
