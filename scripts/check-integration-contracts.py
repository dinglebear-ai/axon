#!/usr/bin/env python3
"""Check integration contracts, or explicitly refresh their documentation copies."""
from __future__ import annotations
import argparse
import json
import sys
from pathlib import Path

COPIES = (
    ("contracts/integration-profile.schema.json", "docs/architecture/integrations/generated/integration-profile.schema.json"),
    ("contracts/base-vocabulary.schema.json", "docs/architecture/integrations/generated/base-vocabulary.schema.json"),
)


def compatible(value):
    return value.get("product") == "axon" and value.get("contract_version") == "1.0.0" and value.get("api_version", {}).get("major") == 1 and str(value.get("server_id", "")).startswith("axon_")


def validate(root: Path) -> list[tuple[Path, bytes]]:
    outputs = []
    for source, destination in COPIES:
        raw = (root / source).read_bytes()
        json.loads(raw)
        outputs.append((root / destination, raw))
    source = json.loads(outputs[0][1])
    required = {"contract_version", "product", "server_id", "product_version", "api_version", "capabilities", "auth", "streams"}
    if set(source["required"]) != required:
        raise ValueError("integration schema required-field set drifted; update the typed contract and compatibility tests together")
    fixtures = root / "contracts/fixtures/integration"
    cases = {name: json.loads((fixtures / name).read_text()) for name in ("valid.json", "wrong-product.json", "unsupported-major.json")}
    if not compatible(cases["valid.json"]) or compatible(cases["wrong-product.json"]) or compatible(cases["unsupported-major.json"]):
        raise ValueError("integration compatibility fixtures failed; inspect the valid/wrong-product/unsupported-major cases")
    redacted = (fixtures / "redacted-error.json").read_text().lower()
    if any(secret in redacted for secret in ("bearer ", "api_key", "access_token", "client_secret", "password")):
        raise ValueError("redacted error fixture contains credential-shaped fields; do not publish it")
    return outputs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--refresh", action="store_true", help="copy validated canonical bytes into documentation")
    mode.add_argument("--check", action="store_true", help="read-only drift check (default)")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.repo_root.resolve()
    try:
        outputs = validate(root)
        stale = [(path, raw) for path, raw in outputs if not path.exists() or path.read_bytes() != raw]
        if stale and not args.refresh:
            for path, _ in stale:
                print(f"integration copy drift: {path.relative_to(root)}; run cargo xtask generated-contracts refresh", file=sys.stderr)
            return 1
        for path, raw in stale:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        print("Axon integration contracts: canonical copies, 3 compatibility fixtures, and redaction fixture passed")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"integration contract validation: {error}; correct canonical inputs before refreshing documentation copies", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
