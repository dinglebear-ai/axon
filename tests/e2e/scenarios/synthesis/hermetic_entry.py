#!/usr/bin/env python3
"""Exercise real synthesis-account HTTP rejection paths on the owned launcher."""
from __future__ import annotations
import argparse
import importlib.util
import json
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[4]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--launcher-descriptor", type=Path, required=True)
    args = parser.parse_args()
    descriptor = json.loads(args.launcher_descriptor.read_text())
    if descriptor["environment"].get("AXON_LLM_BACKEND") != "openai-compat":
        raise RuntimeError("synthesis rejection pack requires the owned OpenAI-compatible backend")
    adapter = load("synthesis_http", ROOT / "scripts/e2e/adapters/http_adapter.py")
    pack = load("synthesis_pack", Path(__file__).with_name("pack.py"))

    def call(method, path, body, authenticated):
        request = (adapter.HttpRequest(method, path, None) if body is None
                   else adapter.json_request(method, path, body))
        token = descriptor["environment"]["AXON_HTTP_TOKEN"] if authenticated else None
        response = adapter.request(descriptor["http_base_url"], token, request, 5)
        return response.status, json.loads(response.body)

    print(json.dumps(pack.run(call), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
