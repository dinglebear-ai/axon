#!/usr/bin/env python3
import json, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "contracts/integration-profile.schema.json"
GENERATED = ROOT / "docs/architecture/integrations/generated/integration-profile.schema.json"
BASE_SOURCE = ROOT / "contracts/base-vocabulary.schema.json"
BASE_GENERATED = ROOT / "docs/architecture/integrations/generated/base-vocabulary.schema.json"
FIXTURES = ROOT / "contracts/fixtures/integration"

def compatible(value):
    return value.get("product") == "axon" and value.get("contract_version") == "1.0.0" and value.get("api_version", {}).get("major") == 1 and str(value.get("server_id", "")).startswith("axon_")

source = json.loads(SOURCE.read_text())
generated = json.loads(GENERATED.read_text())
if source != generated:
    sys.exit("integration contract drift: generated snapshot differs from canonical schema")
if json.loads(BASE_SOURCE.read_text()) != json.loads(BASE_GENERATED.read_text()):
    sys.exit("base vocabulary drift: generated snapshot differs from canonical schema")
required = {"contract_version", "product", "server_id", "product_version", "api_version", "capabilities", "auth", "streams"}
if set(source["required"]) != required:
    sys.exit("integration schema required-field set drifted")
cases = {name: json.loads((FIXTURES / name).read_text()) for name in ("valid.json", "wrong-product.json", "unsupported-major.json")}
if not compatible(cases["valid.json"]) or compatible(cases["wrong-product.json"]) or compatible(cases["unsupported-major.json"]):
    sys.exit("integration compatibility fixtures failed")
redacted = (FIXTURES / "redacted-error.json").read_text().lower()
if any(secret in redacted for secret in ("bearer ", "api_key", "access_token", "client_secret", "password")):
    sys.exit("redacted error fixture contains credential material")
print("Axon integration contract: schema drift, 3 compatibility fixtures, and redaction fixture passed")
