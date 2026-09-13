#!/usr/bin/env python3
"""Fail before an expensive build when live E2E configuration is absent."""
from __future__ import annotations

import json
import os
from collections.abc import Mapping
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def required_names() -> set[str]:
    services = json.loads((ROOT / "config/e2e/live-services.json").read_text())
    names = {"TS_WIF_CLIENT_ID", "TS_WIF_AUDIENCE", "AXON_E2E_EXPECTED_PEERS"}
    for provider in services["providers"]:
        names.update(
            {
                provider["url_env"],
                provider["peer_env"],
                provider["auth_env"],
            }
        )
    return names


def missing_names(env: Mapping[str, str] = os.environ) -> list[str]:
    return sorted(name for name in required_names() if not env.get(name, "").strip())


def main() -> int:
    missing = missing_names()
    if missing:
        raise SystemExit("missing live E2E configuration: " + ", ".join(missing))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
