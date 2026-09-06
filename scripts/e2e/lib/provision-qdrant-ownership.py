#!/usr/bin/env python3
"""Provision and verify durable Qdrant ownership markers after collection setup."""
from __future__ import annotations
import argparse, importlib.util, json, sys
from pathlib import Path

def load(name, filename):
    path = Path(__file__).with_name(filename); spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module); return module
manifest_api = load("axon_e2e_qdrant_manifest", "resource-manifest.py")
providers = load("axon_e2e_qdrant_provider", "provider-adapters.py")

def main():
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path); parser.add_argument("--qdrant-url", required=True)
    parser.add_argument("--report", type=Path, required=True); args = parser.parse_args()
    try:
        header, resources = manifest_api.load(args.manifest); adapter = providers.QdrantAdapter({"base_url": args.qdrant_url}).bind(header, manifest_api)
        qdrant_types = {"collection", "qdrant_alias", "qdrant_snapshot", "point", "payload_index"}
        results = [adapter.provision_ownership_marker(item) for item in resources if item.resource_type in qdrant_types]
        if not results: raise RuntimeError("manifest has no registered Qdrant resource")
        report = {"success": True, "markers": results}
    except Exception as error: report = {"success": False, "fatal": str(error)}
    args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report.get("success") else 2
if __name__ == "__main__": raise SystemExit(main())
