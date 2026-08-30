"""HTTP and Qdrant provider adapters."""
from __future__ import annotations

from axon_e2e_provider_common import *
from axon_e2e_provider_common import segment as _segment

class ExactHttpAdapter:
    """HTTP adapter whose configured paths receive one URL-encoded exact ID."""

    def __init__(self, config: dict[str, Any]):
        self.base = str(config["base_url"]).rstrip("/")
        self.paths = dict(config["resources"])
        self.token = str(config.get("token", "")); self.timeout = float(config.get("timeout_seconds", 5))
        self.round_trips = 0; self.deadline: float | None = None

    def set_deadline(self, deadline: float) -> None: self.deadline = deadline

    def _url(self, resource: Any, operation: str) -> str:
        template = self.paths.get(resource.resource_type, {}).get(operation)
        if not isinstance(template, str) or template.count("{identity}") != 1:
            raise ProviderError(f"no exact {operation} endpoint for {resource.resource_type}")
        return self.base + template.replace("{identity}", _segment(resource.identity))

    def _request(self, url: str, method: str, payload: Any = None) -> tuple[int, Any]:
        headers = {"Accept": "application/json"}
        if self.token: headers["Authorization"] = f"Bearer {self.token}"
        data = None
        if payload is not None:
            data = json.dumps(payload).encode(); headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, method=method, headers=headers)
        try:
            self.round_trips += 1
            timeout = self.timeout if self.deadline is None else min(self.timeout, max(.05, self.deadline - time.monotonic()))
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read(); return response.status, json.loads(raw) if raw else {}
        except urllib.error.HTTPError as error:
            try:
                if error.code == 404: return 404, {}
                raise ProviderError(f"HTTP {method} failed with status {error.code}") from error
            finally:error.close()
        except (urllib.error.URLError, TimeoutError, socket.timeout) as error:
            raise ProviderError(f"HTTP {method} state is unknown: {error}") from error

    def marker(self, resource: Any) -> dict[str, Any] | None:
        status, body = self._request(self._url(resource, "get"), "GET")
        if status == 404: return None
        marker = body.get("ownership") if isinstance(body, dict) else None
        return marker if isinstance(marker, dict) else None

    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() >= deadline: raise TimeoutError("provider deadline exceeded")
        status, _ = self._request(self._url(resource, "delete"), "DELETE")
        return "absent" if status == 404 else "removed"

    def exists(self, resource: Any) -> bool:
        status, _ = self._request(self._url(resource, "get"), "GET"); return status != 404

    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        return [(resource, self.delete(resource, deadline)) for resource in resources]
    @staticmethod
    def batch_capability(_resource_type: str) -> str: return "unbatchable-provider-contract"


class QdrantAdapter(ExactHttpAdapter):
    """Standard Qdrant REST operations with manifest-bound ownership.

    Qdrant has no collection metadata ownership field.  The signed manifest
    checkpoint is therefore the ownership authority, while provider reads prove
    that the exact collection/alias/snapshot/point/index still exists.
    """

    def __init__(self, config: dict[str, Any]):
        if config.get("tenant_enforced") is not True or not str(config.get("owned_prefix", "")).startswith("axon_e2e_"):
            raise ProviderError("Qdrant E2E requires a dedicated tenant endpoint and owned_prefix enforcement")
        self.owned_prefix = str(config["owned_prefix"])
        super().__init__({**config, "resources": {"collection": {
            "get": "/collections/{identity}", "delete": "/collections/{identity}"}}})

    @staticmethod
    def _collection(resource: Any) -> str:
        value = resource.metadata.get("collection")
        if not isinstance(value, str) or not value: raise ProviderError(f"{resource.resource_type} requires collection metadata")
        return value

    def _state(self, resource: Any) -> bool:
        kind, identity = resource.resource_type, resource.identity
        if kind == "collection":
            return self._request(f"{self.base}/collections/{_segment(identity)}", "GET")[0] != 404
        if kind == "qdrant_alias":
            status, body = self._request(f"{self.base}/aliases", "GET")
            if status == 404: return False
            aliases = body.get("result", {}).get("aliases", [])
            return any(item.get("alias_name") == identity for item in aliases if isinstance(item, dict))
        collection = self._collection(resource); root = f"{self.base}/collections/{_segment(collection)}"
        if kind == "qdrant_snapshot":
            status, body = self._request(f"{root}/snapshots", "GET")
            return status != 404 and any(item.get("name") == identity for item in body.get("result", []))
        if kind == "point":
            status, body = self._request(f"{root}/points", "POST", {"ids": [identity], "with_payload": True})
            return status != 404 and bool(body.get("result"))
        if kind == "payload_index":
            status, body = self._request(f"{root}", "GET")
            schema = body.get("result", {}).get("payload_schema", {}) if status != 404 else {}
            return identity in schema
        raise ProviderError(f"unsupported Qdrant resource: {kind}")

    def _provider_state(self, resource: Any) -> Any:
        """Return the stable provider-native identity state used against reuse."""
        kind, identity = resource.resource_type, resource.identity
        if kind == "collection":
            status, body = self._request(f"{self.base}/collections/{_segment(identity)}", "GET")
            if status == 404: return None
            result = body.get("result", {}); config = result.get("config", {})
            return {"config": config, "optimizer_status": result.get("optimizer_status")}
        if kind == "qdrant_alias":
            _, body = self._request(f"{self.base}/aliases", "GET")
            return next((item for item in body.get("result", {}).get("aliases", []) if item.get("alias_name") == identity), None)
        collection = self._collection(resource); root = f"{self.base}/collections/{_segment(collection)}"
        if kind == "qdrant_snapshot":
            _, body = self._request(f"{root}/snapshots", "GET")
            return next((item for item in body.get("result", []) if item.get("name") == identity), None)
        if kind == "payload_index":
            _, body = self._request(root, "GET")
            return body.get("result", {}).get("payload_schema", {}).get(identity)
        if kind == "point":
            _, body = self._request(f"{root}/points", "POST", {"ids": [identity], "with_payload": True, "with_vector": False})
            result = body.get("result", [])
            if not result: return None
            point = dict(result[0]); payload = dict(point.get("payload", {})); payload.pop("axon_e2e_ownership", None)
            point["payload"] = payload; point.pop("vector", None); return point
        raise ProviderError(f"unsupported Qdrant state resource: {kind}")

    @staticmethod
    def _state_digest(state: Any) -> str:
        return hashlib.sha256(json.dumps(state, sort_keys=True, separators=(",", ":")).encode()).hexdigest()

    def marker(self, resource: Any) -> dict[str, Any] | None:
        if not self._state(resource): return None
        collection = resource.identity if resource.resource_type == "collection" else self._collection(resource)
        if resource.resource_type == "point": point_id = resource.identity
        else:
            try: point_id = self.manifest_api.qdrant_ownership_point(self.header, resource)["id"]
            except Exception as error: raise ProviderError(f"Qdrant resource lacks a durable ownership marker point binding: {error}") from error
        status, body = self._request(
            f"{self.base}/collections/{_segment(collection)}/points", "POST",
            {"ids": [point_id], "with_payload": True, "with_vector": False},
        )
        if status == 404 or not body.get("result"): return None
        payload = body["result"][0].get("payload", {})
        marker = payload.get("axon_e2e_ownership") if isinstance(payload, dict) else None
        if not isinstance(marker, dict): raise ProviderError("Qdrant ownership marker payload is absent")
        generation = resource.metadata.get("ownership_generation")
        if not isinstance(generation, str) or marker.get("generation") != generation:
            raise ProviderError("Qdrant ownership generation changed")
        return {key: value for key, value in marker.items() if key not in {"generation", "provider_state_sha256"}}

    def bind(self, header: Any, manifest_api: Any) -> "QdrantAdapter":
        self.header, self.manifest_api = header, manifest_api; return self

    def exists(self, resource: Any) -> bool: return self._state(resource)

    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() >= deadline: raise TimeoutError("provider deadline exceeded")
        if not self._state(resource): return "absent"
        kind, identity = resource.resource_type, resource.identity
        if kind == "collection": self._request(f"{self.base}/collections/{_segment(identity)}", "DELETE")
        elif kind == "qdrant_alias": self._request(f"{self.base}/collections/aliases", "POST", {
            "actions": [{"delete_alias": {"alias_name": identity}}]})
        else:
            collection = self._collection(resource); root = f"{self.base}/collections/{_segment(collection)}"
            if kind == "qdrant_snapshot": self._request(f"{root}/snapshots/{_segment(identity)}", "DELETE")
            elif kind == "point": self._request(f"{root}/points/delete?wait=true", "POST", {"points": [identity]})
            elif kind == "payload_index": self._request(f"{root}/index/{_segment(identity)}?wait=true", "DELETE")
        return "removed"

    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        """Use Qdrant's actual multi-point and multi-alias operations."""
        if not resources: return []
        kind = resources[0].resource_type
        if any(item.resource_type != kind for item in resources): raise ProviderError("mixed Qdrant batch")
        present = [item for item in resources if self._state(item)]
        if kind == "point" and present:
            collection = self._collection(present[0])
            if any(self._collection(item) != collection for item in present):
                return [(item, self.delete(item, deadline)) for item in resources]
            self._request(f"{self.base}/collections/{_segment(collection)}/points/delete?wait=true", "POST",
                          {"points": [item.identity for item in present]})
        elif kind == "qdrant_alias" and present:
            self._request(f"{self.base}/collections/aliases", "POST", {"actions": [
                {"delete_alias": {"alias_name": item.identity}} for item in present]})
        else:
            return [(item, self.delete(item, deadline)) for item in resources]
        keys = {(item.resource_type, item.identity) for item in present}
        return [(item, "removed" if (item.resource_type, item.identity) in keys else "absent") for item in resources]

    @staticmethod
    def batch_capability(resource_type: str) -> str:
        return "provider-batch" if resource_type in {"point", "qdrant_alias"} else "unbatchable-provider-contract"

    def provision_ownership_marker(self, resource: Any) -> dict[str, Any]:
        """Actually provision and read back ownership for every Qdrant class."""
        if hasattr(self.manifest_api, "write_setup_intent"): self.manifest_api.write_setup_intent(self.header, resource)
        collection = resource.identity if resource.resource_type == "collection" else self._collection(resource)
        status, body = self._request(f"{self.base}/collections/{_segment(collection)}", "GET")
        if status == 404: raise ProviderError("cannot provision marker before collection creation")
        vectors = body.get("result", {}).get("config", {}).get("params", {}).get("vectors")
        def dense(spec: Any) -> list[float]:
            size = spec.get("size") if isinstance(spec, dict) else None
            if not isinstance(size, int) or size < 1: raise ProviderError("Qdrant collection vector size is unavailable")
            return [0.0] * size
        if isinstance(vectors, dict) and isinstance(vectors.get("size"), int): vector: Any = dense(vectors)
        elif isinstance(vectors, dict) and vectors: vector = {name: dense(spec) for name, spec in vectors.items()}
        else: raise ProviderError("Qdrant marker requires at least one dense vector")
        state = self._provider_state(resource)
        if state is None: raise ProviderError("cannot provision ownership before exact Qdrant resource creation")
        ownership = {**self.manifest_api.provider_marker(self.header, resource),
                     "generation": resource.metadata.get("ownership_generation")}
        if resource.resource_type == "point":
            point_id = resource.identity
            self._request(f"{self.base}/collections/{_segment(collection)}/points/payload?wait=true", "POST",
                          {"payload": {"axon_e2e_ownership": ownership}, "points": [point_id]})
        else:
            point = self.manifest_api.qdrant_ownership_point(self.header, resource); point_id = point["id"]
            point["vector"] = vector; point["payload"]["axon_e2e_ownership"] = ownership
            self._request(f"{self.base}/collections/{_segment(collection)}/points?wait=true", "PUT", {"points": [point]})
        marker = self.marker(resource)
        self.manifest_api.verify_marker(self.header, resource, marker or {})
        return {"collection": collection, "point_id": point_id,
                "generation": resource.metadata["ownership_generation"]}

    def create_and_provision(self, resource: Any) -> dict[str, Any]:
        """Persist intent, create through the standard API, then mark immediately."""
        self.manifest_api.write_setup_intent(self.header, resource)
        kind, identity = resource.resource_type, resource.identity
        collection = identity if kind == "collection" else self._collection(resource)
        root = f"{self.base}/collections/{_segment(collection)}"
        if kind == "collection":
            payload = resource.metadata.get("create_payload")
            if not isinstance(payload, dict) or "vectors" not in payload:
                raise ProviderError("collection creation requires create_payload.vectors")
            self._request(root, "PUT", payload)
        elif kind == "qdrant_alias":
            self._request(f"{self.base}/collections/aliases", "POST", {"actions": [
                {"create_alias": {"collection_name": collection, "alias_name": identity}}]})
        elif kind == "qdrant_snapshot":
            _status, body = self._request(f"{root}/snapshots", "POST")
            created = body.get("result", body).get("name") if isinstance(body.get("result", body), dict) else None
            if created != identity: raise ProviderError("Qdrant snapshot provider did not return registered identity")
        elif kind == "point":
            point = resource.metadata.get("point")
            if not isinstance(point, dict) or str(point.get("id")) != identity:
                raise ProviderError("point creation requires exact metadata.point.id")
            self._request(f"{root}/points?wait=true", "PUT", {"points": [point]})
        elif kind == "payload_index":
            schema = resource.metadata.get("field_schema")
            if not isinstance(schema, dict): raise ProviderError("payload-index creation requires field_schema")
            self._request(f"{root}/index/{_segment(identity)}?wait=true", "PUT", schema)
        else: raise ProviderError(f"unsupported Qdrant setup resource: {kind}")
        try: return self.provision_ownership_marker(resource)
        except BaseException:
            # The signed creating intent authorizes rollback of this exact ID.
            self.recover_creating(resource, time.monotonic() + self.timeout)
            raise

    def recover_creating(self, resource: Any, deadline: float) -> str:
        """Recover only the exact generation recorded before setup began."""
        self.manifest_api.verify_setup_intent(self.header, resource)
        if not (resource.identity.startswith(self.owned_prefix) or
                self._collection(resource).startswith(self.owned_prefix)):
            raise ProviderError("Qdrant recovery target is outside the dedicated E2E namespace")
        return self.delete(resource, deadline)

    def snapshot_shared(self, owned: set[tuple[str, str]]) -> dict[str, Any]:
        """Snapshot all unowned Qdrant config/index/snapshot/point state."""
        _, collections = self._request(f"{self.base}/collections", "GET")
        _, aliases = self._request(f"{self.base}/aliases", "GET")
        owned_collections = {identity for kind, identity in owned if kind == "collection"}
        owned_aliases = {identity for kind, identity in owned if kind == "qdrant_alias"}
        names = sorted(item.get("name") for item in collections.get("result", {}).get("collections", [])
                       if item.get("name", "").startswith(self.owned_prefix) and item.get("name") not in owned_collections)
        details = {}
        for name in names:
            _, collection = self._request(f"{self.base}/collections/{_segment(name)}", "GET")
            _, snapshots = self._request(f"{self.base}/collections/{_segment(name)}/snapshots", "GET")
            points, offset = [], None
            while True:
                request = {"limit": 256, "with_payload": True, "with_vector": True}
                if offset is not None: request["offset"] = offset
                _, page = self._request(f"{self.base}/collections/{_segment(name)}/points/scroll", "POST", request)
                result = page.get("result", {}); points.extend(result.get("points", [])); offset = result.get("next_page_offset")
                if offset is None: break
            details[name] = {"collection": collection.get("result"), "snapshots": snapshots.get("result", []),
                             "points_sha256": self._state_digest(sorted(points, key=lambda item: str(item.get("id")))),
                             "point_count": len(points)}
        return {
            "collections": details,
            "aliases": sorted((item.get("alias_name"), item.get("collection_name"))
                              for item in aliases.get("result", {}).get("aliases", [])
                              if item.get("alias_name", "").startswith(self.owned_prefix)
                              and item.get("alias_name") not in owned_aliases),
        }

    def discover_unregistered(self, run_id: str, owned: set[tuple[str, str]]) -> list[dict[str, str]]:
        snapshot = self.snapshot_shared(owned); found = []
        for identity in snapshot["collections"]:
            if identity == run_id or identity.startswith(run_id + "_"):
                found.append({"resource_type": "collection", "identity": identity})
        for alias, _collection in snapshot["aliases"]:
            if alias == run_id or alias.startswith(run_id + "_"):
                found.append({"resource_type": "qdrant_alias", "identity": alias})
        return found
