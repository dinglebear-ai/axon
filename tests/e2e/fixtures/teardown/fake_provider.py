"""In-memory exact-identity provider used only by teardown contract tests."""

from __future__ import annotations

import threading
import time
from typing import Any


class FakeProvider:
    def __init__(self, manifest_api: Any, header: Any, resources: list[Any], *, unknown: bool = False,
                 fail_delete: set[tuple[str, str]] | None = None):
        self.lock = threading.Lock(); self.unknown = unknown; self.fail_delete = fail_delete or set()
        self.deleted: list[tuple[str, str]] = []
        self.batch_calls = 0
        self.state = {}
        for item in resources:
            self.state[(item.resource_type, item.identity)] = {
                "exists": True, "marker": manifest_api.provider_marker(header, item,
                    attempt=int(item.metadata.get("attempt", 1)), owner=str(item.metadata.get("owner", "axon-e2e")))
            }

    def marker(self, resource):
        with self.lock: return dict(self.state[(resource.resource_type, resource.identity)]["marker"])
    def delete(self, resource, deadline):
        if time.monotonic() > deadline: raise TimeoutError("fake deadline")
        with self.lock:
            if (resource.resource_type, resource.identity) in self.fail_delete: raise RuntimeError("injected provider outage")
            item = self.state[(resource.resource_type, resource.identity)]
            if not item["exists"]: return "absent"
            item["exists"] = False; self.deleted.append((resource.resource_type, resource.identity)); return "removed"
    def exists(self, resource):
        if self.unknown: return None
        with self.lock: return self.state[(resource.resource_type, resource.identity)]["exists"]
    def delete_batch(self, resources, deadline):
        self.batch_calls += 1
        return [(resource, self.delete(resource, deadline)) for resource in resources]
