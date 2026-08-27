"""Structured argv, Docker, and Compose provider adapters."""
from __future__ import annotations

from axon_e2e_provider_common import *

class ArgvAdapter:
    """Exact argv operations with no shell, glob, or interpolated command text."""

    def __init__(self, config: dict[str, Any]):
        self.binary = str(config["binary"]); self.operations = dict(config["resources"])
        self.timeout = float(config.get("timeout_seconds", 10)); self.round_trips = 0; self.deadline: float | None = None
    def set_deadline(self, deadline: float) -> None: self.deadline = deadline

    def _argv(self, resource: Any, operation: str) -> list[str]:
        template = self.operations.get(resource.resource_type, {}).get(operation)
        if not isinstance(template, list) or sum(str(item).count("{identity}") for item in template) != 1:
            raise ProviderError(f"no exact argv {operation} for {resource.resource_type}")
        return [self.binary, *(str(item).replace("{identity}", resource.identity) for item in template)]

    def _run(self, resource: Any, operation: str) -> subprocess.CompletedProcess[str]:
        self.round_trips += 1
        try:
            timeout = self.timeout if self.deadline is None else min(self.timeout, max(.05, self.deadline - time.monotonic()))
            return subprocess.run(self._argv(resource, operation), capture_output=True, text=True,
                                  timeout=timeout, check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ProviderError(f"argv {operation} state is unknown: {error}") from error

    def marker(self, resource: Any) -> dict[str, Any] | None:
        result = self._run(resource, "inspect")
        if result.returncode == 1: return None
        if result.returncode: raise ProviderError("inspect failed")
        try: body = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("inspect returned malformed JSON") from error
        if isinstance(body, list) and len(body) == 1: body = body[0]
        if not isinstance(body, dict): return None
        marker = body.get("ownership")
        if not isinstance(marker, dict):
            labels = body.get("Config", {}).get("Labels") if resource.resource_type == "container" else body.get("Labels")
            encoded = labels.get("axon.e2e.ownership") if isinstance(labels, dict) else None
            if isinstance(encoded, str):
                try: marker = json.loads(encoded)
                except json.JSONDecodeError as error: raise ProviderError("ownership label is malformed") from error
        if isinstance(marker, dict) and "generation" in marker:
            if marker.get("generation") != resource.metadata.get("ownership_generation"):
                raise ProviderError("Docker ownership label generation changed")
            marker = {key: value for key, value in marker.items() if key != "generation"}
        return marker if isinstance(marker, dict) else None
    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() >= deadline: raise TimeoutError("provider deadline exceeded")
        result = self._run(resource, "delete")
        if result.returncode not in (0, 1): raise ProviderError("delete failed")
        return "removed" if result.returncode == 0 else "absent"
    def exists(self, resource: Any) -> bool:
        result = self._run(resource, "inspect")
        if result.returncode not in (0, 1): raise ProviderError("inspect state is unknown")
        return result.returncode == 0
    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        return [(resource, self.delete(resource, deadline)) for resource in resources]
    @staticmethod
    def batch_capability(_resource_type: str) -> str: return "unbatchable-cli-contract"
    def snapshot_shared(self, owned: set[tuple[str, str]]) -> dict[str, Any]:
        if Path(self.binary).name != "docker": raise ProviderError("shared snapshot is unsupported for this CLI")
        commands = {"container": [self.binary, "container", "ls", "--all", "--format", "{{.Names}}"],
                    "network": [self.binary, "network", "ls", "--format", "{{.Name}}"],
                    "volume": [self.binary, "volume", "ls", "--format", "{{.Name}}"]}
        output = {}
        for kind in self.operations:
            result = subprocess.run(commands[kind], capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
            if result.returncode: raise ProviderError(f"Docker {kind} shared snapshot failed")
            excluded = {identity for resource_type, identity in owned if resource_type == kind}
            output[kind] = sorted(value for value in result.stdout.splitlines() if value and value not in excluded)
        return output

    def discover_unregistered(self, run_id: str, owned: set[tuple[str, str]]) -> list[dict[str, str]]:
        return [{"resource_type": kind, "identity": identity}
                for kind, identities in self.snapshot_shared(owned).items() for identity in identities
                if identity == run_id or identity.startswith(run_id + "_")]


class ManifestBoundArgvAdapter(ArgvAdapter):
    """Exact CLI inspection plus ownership from the signed run manifest."""

    def __init__(self, config: dict[str, Any], header: Any, manifest_api: Any):
        super().__init__(config); self.header, self.manifest_api = header, manifest_api
        self.identity_fields = config.get("identity_fields", {})

    def _identity_state(self, resource: Any, body: Any) -> dict[str, Any]:
        fields = self.identity_fields.get(resource.resource_type)
        if not isinstance(fields, list) or not fields:
            raise ProviderError(f"{resource.resource_type} requires immutable identity_fields")
        if not isinstance(body, dict) or any(field not in body for field in fields):
            raise ProviderError("provider response lacks immutable identity fields")
        return {field: body[field] for field in fields}

    def marker(self, resource: Any) -> dict[str, Any] | None:
        result = self._run(resource, "inspect")
        if result.returncode == 1: return None
        if result.returncode: raise ProviderError("inspect failed")
        try: body = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("inspect returned malformed JSON") from error
        return self.manifest_api.verify_provider_ledger(self.header, resource, self._identity_state(resource, body))

    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        result = self._run(resource, "inspect")
        if result.returncode: raise ProviderError("cannot bind ownership to an absent provider resource")
        try: body = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("inspect returned malformed JSON") from error
        return self.manifest_api.write_provider_ledger(self.header, resource, self._identity_state(resource, body))
    def create_and_provision(self, resource: Any) -> dict[str, Any]:
        if hasattr(self.manifest_api, "write_setup_intent"): self.manifest_api.write_setup_intent(self.header, resource)
        result = self._run(resource, "create")
        if result.returncode: raise ProviderError(f"{resource.resource_type} creation failed")
        return self.provision_ownership(resource)
    def recover_creating(self, resource: Any, deadline: float) -> str:
        self.manifest_api.verify_setup_intent(self.header, resource)
        return self.delete(resource, deadline)
    def snapshot_shared(self, owned: set[tuple[str, str]]) -> dict[str, Any]:
        output = {}
        for kind in self.operations:
            family = "uploads" if kind == "upload" else kind
            try: result = subprocess.run([self.binary, "--json", family, "list"], capture_output=True, text=True,
                                         timeout=self.timeout, check=False, shell=False)
            except (OSError, subprocess.TimeoutExpired) as error: raise ProviderError(f"Axon {family} operator snapshot unknown: {error}") from error
            if result.returncode: raise ProviderError(f"Axon {family} operator snapshot failed")
            try: body = json.loads(result.stdout)
            except json.JSONDecodeError as error: raise ProviderError(f"Axon {family} operator snapshot malformed") from error
            items = body.get("items", body) if isinstance(body, dict) else body
            excluded = {identity for resource_type, identity in owned if resource_type == kind}
            id_key = "upload_id" if kind == "upload" else "watch_id"
            output[kind] = sorted([item for item in items if item.get(id_key) not in excluded],
                                  key=lambda item: str(item.get(id_key)))
        return output
    def discover_unregistered(self, run_id: str, owned: set[tuple[str, str]]) -> list[dict[str, str]]:
        snapshot = self.snapshot_shared(owned); found = []
        for kind, items in snapshot.items():
            id_key = "upload_id" if kind == "upload" else "watch_id"
            for item in items:
                identity = str(item.get(id_key, ""))
                if identity == run_id or identity.startswith(run_id + "_"):
                    found.append({"resource_type": kind, "identity": identity})
        return found


class DockerAdapter(ArgvAdapter):
    """Docker resources whose immutable ownership label is set at creation."""
    def __init__(self, config: dict[str, Any], header: Any, manifest_api: Any):
        super().__init__(config); self.header, self.manifest_api = header, manifest_api
    def _label(self, resource: Any) -> str:
        generation = resource.metadata.get("ownership_generation")
        if not isinstance(generation, str) or len(generation) < 32: raise ProviderError("Docker creation requires strong ownership_generation")
        return json.dumps({**self.manifest_api.provider_marker(self.header, resource), "generation": generation},
                          sort_keys=True, separators=(",", ":"))
    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        if hasattr(self.manifest_api, "write_setup_intent"): self.manifest_api.write_setup_intent(self.header, resource)
        label = f"axon.e2e.ownership={self._label(resource)}"; kind = resource.resource_type
        if kind == "container":
            image = resource.metadata.get("image")
            if not isinstance(image, str) or not image: raise ProviderError("container setup requires pinned image")
            argv = [self.binary, "container", "create", "--label", label, "--name", resource.identity, image]
        elif kind in {"network", "volume"}: argv = [self.binary, kind, "create", "--label", label, resource.identity]
        else: raise ProviderError("unsupported Docker setup resource")
        try: result = subprocess.run(argv, capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error: raise ProviderError(f"Docker setup state unknown: {error}") from error
        if result.returncode: raise ProviderError(f"Docker {kind} creation failed")
        marker = self.marker(resource)
        if marker is None: raise ProviderError("Docker ownership label missing after creation")
        self.manifest_api.verify_marker(self.header, resource, marker)
        return {"resource_type": kind, "identity": resource.identity, "generation": resource.metadata["ownership_generation"]}
    def recover_creating(self, resource: Any, deadline: float) -> str:
        self.manifest_api.verify_setup_intent(self.header, resource)
        return self.delete(resource, deadline)


class DockerComposeAdapter:
    """Delete only an exact, manifest-owned Compose project."""

    def __init__(self, config: dict[str, Any], header: Any, manifest_api: Any):
        self.binary = str(config.get("binary", "docker")); self.timeout = float(config.get("timeout_seconds", 30))
        self.header, self.manifest_api = header, manifest_api; self.round_trips = 0; self.deadline = None
    def set_deadline(self, deadline: float) -> None: self.deadline = deadline
    def _run(self, resource: Any, operation: str) -> subprocess.CompletedProcess[str]:
        if operation == "inspect": argv = [self.binary, "compose", "-p", resource.identity, "ps", "--all", "--format", "json"]
        else: argv = [self.binary, "compose", "-p", resource.identity, "down", "--remove-orphans", "--volumes"]
        self.round_trips += 1
        try:
            timeout = min(self.timeout, max(.1, (self.deadline or time.monotonic() + self.timeout) - time.monotonic()))
            return subprocess.run(argv, capture_output=True, text=True, timeout=timeout, check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error: raise ProviderError(f"compose {operation} state is unknown: {error}") from error
    def exists(self, resource: Any) -> bool:
        result = self._run(resource, "inspect")
        if result.returncode: raise ProviderError("compose inspect state is unknown")
        return bool(result.stdout.strip()) and result.stdout.strip() not in {"[]", "null"}
    def marker(self, resource: Any) -> dict[str, Any] | None:
        result = self._run(resource, "inspect")
        if result.returncode: raise ProviderError("compose inspect state is unknown")
        if not result.stdout.strip() or result.stdout.strip() in {"[]", "null"}: return None
        try: state = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("compose inspect returned malformed JSON") from error
        return self.manifest_api.verify_provider_ledger(self.header, resource, self._identity_state(state))
    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        result = self._run(resource, "inspect")
        if result.returncode or not result.stdout.strip(): raise ProviderError("cannot bind absent Compose project")
        try: state = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("compose inspect returned malformed JSON") from error
        return self.manifest_api.write_provider_ledger(self.header, resource, self._identity_state(state))
    def create_and_provision(self, resource: Any) -> dict[str, Any]:
        self.manifest_api.write_setup_intent(self.header, resource)
        argv = [self.binary, "compose", "-p", resource.identity, "up", "-d"]
        result = subprocess.run(argv, capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
        if result.returncode: raise ProviderError("docker compose creation failed")
        return self.provision_ownership(resource)
    def recover_creating(self, resource: Any, deadline: float) -> str:
        self.manifest_api.verify_setup_intent(self.header, resource)
        return self.delete(resource, deadline)

    @staticmethod
    def _identity_state(state: Any) -> list[dict[str, Any]]:
        items = state if isinstance(state, list) else [state]
        stable = []
        for item in items:
            if not isinstance(item, dict) or not item.get("ID") or not item.get("Name"):
                raise ProviderError("Compose identity requires immutable container ID and Name")
            stable.append({"ID": item["ID"], "Name": item["Name"], "Project": item.get("Project")})
        return sorted(stable, key=lambda item: (str(item["Name"]), str(item["ID"])))
    def delete(self, resource: Any, deadline: float) -> str:
        if not self.exists(resource): return "absent"
        result = self._run(resource, "delete")
        if result.returncode: raise ProviderError("docker compose down failed")
        return "removed"
    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        return [(resource, self.delete(resource, deadline)) for resource in resources]
    @staticmethod
    def batch_capability(_resource_type: str) -> str: return "unbatchable-compose-project"
    def snapshot_shared(self, owned: set[tuple[str, str]]) -> dict[str, Any]:
        try: result = subprocess.run([self.binary, "compose", "ls", "--all", "--format", "json"],
                                     capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error: raise ProviderError(f"Compose operator snapshot unknown: {error}") from error
        if result.returncode: raise ProviderError("Compose operator snapshot failed")
        try: projects = json.loads(result.stdout or "[]")
        except json.JSONDecodeError as error: raise ProviderError("Compose operator snapshot malformed") from error
        excluded = {identity for kind, identity in owned if kind == "compose_project"}
        return {"projects": sorted((item.get("Project", item.get("Name")), item.get("Status"), item.get("ConfigFiles"))
                                   for item in projects if item.get("Project", item.get("Name")) not in excluded)}
