#!/usr/bin/env python3
"""Public loader for structured, exact-identity E2E provider adapters.

This filename remains hyphenated for compatibility with teardown scripts that
load it directly. Implementations live in cohesive sibling modules.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


_LIB_DIR = str(Path(__file__).resolve().parent)
_ADDED_LIB_DIR = _LIB_DIR not in sys.path
if _ADDED_LIB_DIR:
    sys.path.insert(0, _LIB_DIR)
try:
    from axon_e2e_provider_common import ProviderError
    from axon_e2e_provider_qdrant import ExactHttpAdapter, QdrantAdapter
    from axon_e2e_provider_argv import (
        ArgvAdapter,
        DockerAdapter,
        DockerComposeAdapter,
        ManifestBoundArgvAdapter,
    )
    from axon_e2e_provider_state import (
        DurableStateAdapter,
        FileStateAdapter,
        TailscaleAdapter,
    )
finally:
    if _ADDED_LIB_DIR:
        sys.path.remove(_LIB_DIR)


def build(config_path: Path, header: Any, manifest_api: Any) -> dict[str, Any]:
    config = json.loads(config_path.read_text())
    adapters: dict[str, Any] = {}
    for name, item in config.get("providers", {}).items():
        if item.get("kind") == "http": adapter = ExactHttpAdapter(item)
        elif item.get("kind") == "qdrant": adapter = QdrantAdapter(item).bind(header, manifest_api)
        elif item.get("kind") == "argv": adapter = ArgvAdapter(item)
        elif item.get("kind") == "docker": adapter = DockerAdapter(item, header, manifest_api)
        elif item.get("kind") == "manifest-argv": adapter = ManifestBoundArgvAdapter(item, header, manifest_api)
        elif item.get("kind") == "docker-compose": adapter = DockerComposeAdapter(item, header, manifest_api)
        elif item.get("kind") == "durable-state": adapter = DurableStateAdapter(header, manifest_api)
        elif item.get("kind") == "owned-state": adapter = FileStateAdapter(header, manifest_api)
        elif item.get("kind") == "tailscale": adapter = TailscaleAdapter(item, header, manifest_api)
        else: raise ProviderError(f"unsupported provider adapter kind: {name}")
        for resource_type in item.get("resource_types", []):
            if resource_type in adapters: raise ProviderError(f"duplicate adapter for {resource_type}")
            adapters[resource_type] = adapter
    return adapters
