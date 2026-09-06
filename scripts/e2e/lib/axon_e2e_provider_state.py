"""Durable-state, owned-file, and Tailscale provider adapters."""
from __future__ import annotations
from contextlib import closing

from axon_e2e_provider_common import *

class GatewayLeaseAdapter:
    """Canonical teardown adapter for bearer-authenticated disposable provider leases."""
    def __init__(self, header: Any, manifest_api: Any):self.header,self.manifest_api=header,manifest_api;self.round_trips=0
    def _request(self,resource,method="GET",payload=None):
        base=os.environ.get(resource.metadata.get("base_url_env",""),"").rstrip("/");token=os.environ.get(resource.metadata.get("token_env",""),"")
        if not base.startswith("https://") or not token:raise ProviderError("gateway lease endpoint/auth is unavailable")
        url=base+"/v1/e2e/leases/"+segment(resource.metadata["lease_id"]);data=None if payload is None else json.dumps(payload).encode()
        request=urllib.request.Request(url,data=data,method=method,headers={"Authorization":f"Bearer {token}","Content-Type":"application/json"});self.round_trips+=1
        try:
            with urllib.request.urlopen(request,timeout=10) as response:return json.load(response)
        except urllib.error.HTTPError as error:
            try:
                if error.code==404:return None
                raise ProviderError("gateway lease request failed") from error
            finally:error.close()
    def marker(self,resource):
        body=self._request(resource)
        if body is None:return None
        expected=(resource.metadata["lease_id"],resource.metadata["namespace"],resource.metadata["gateway_owner"],resource.metadata["github_run_id"],resource.metadata["run_attempt"])
        if (body.get("lease_id"),body.get("namespace"),body.get("owner"),body.get("run_id"),body.get("run_attempt"))!=expected:raise ProviderError("gateway provider-visible ownership changed")
        state={key:body[key] for key in ("lease_id","namespace","provider","owner","run_id","run_attempt")}
        return self.manifest_api.verify_provider_ledger(self.header,resource,state)
    def delete(self,resource,_deadline):
        body=self._request(resource,"DELETE",{"namespace":resource.metadata["namespace"],"owner":resource.metadata["gateway_owner"],"residual_audit":True})
        if body!={"status":"deleted","residuals":[]}:raise ProviderError("gateway exhaustive residual audit failed")
        return "deleted-and-audited"
    def exists(self,resource):return self._request(resource) is not None
    def snapshot_shared(self,owned):return {"boundary":"dedicated-disposable-provider-gateway","owned_excluded":sorted(identity for kind,identity in owned if kind=="provider_reservation")}

class DurableStateAdapter:
    """Exact cleanup/audit against the isolated SQLite or owned state path."""
    absent_is_clean = True
    TABLES = {
        "job": ("jobs", ("job_id",)), "job_attempt": ("job_attempts", ("attempt_id",)),
        "job_stage": ("job_stages", ("stage_id",)), "job_event": ("job_events", ("event_id",)),
        "job_heartbeat": ("job_heartbeats", ("job_id", "attempt")), "job_artifact": ("job_artifacts", ("artifact_id",)),
        "config_snapshot": ("config_snapshots", ("config_snapshot_id",)),
        "watch": ("axon_source_watches", ("watch_id",)), "watch_run": ("axon_source_watch_runs", ("id",)),
        "provider_reservation": ("provider_reservations", ("reservation_id",)),
        "source": ("sources", ("source_id",)), "source_generation": ("source_generations", ("source_id", "generation")),
        "source_manifest": ("source_manifests", ("source_id", "generation")),
        "source_item": ("source_items", ("source_id", "generation", "source_item_key")),
        "document_status": ("document_status", ("document_id",)), "cleanup_debt": ("cleanup_debt", ("debt_id",)),
        "source_lease": ("leases", ("lease_id",)),
        "graph_node": ("graph_nodes", ("node_id",)), "graph_edge": ("graph_edges", ("edge_id",)),
        # evidence_id is generated as a globally opaque identity even though
        # SQLite's relational primary key also carries edge_id.
        "graph_evidence": ("graph_evidence", ("evidence_id",)),
        "graph_alias": ("graph_aliases", ("alias_kind", "alias_value")),
        "graph_conflict": ("graph_conflicts", ("conflict_id",)),
        "memory_record": ("memory_records", ("memory_id",)), "memory_link": ("memory_links", ("id",)),
        "memory_reinforcement": ("memory_reinforcement", ("id",)), "memory_review": ("memory_reviews", ("id",)),
        "memory_node": ("axon_memory_nodes", ("node_id",)), "memory_edge": ("axon_memory_edges", ("edge_id",)),
        "observe_event": ("axon_observe_events", ("event_id",)),
        "observe_heartbeat": ("axon_observe_heartbeats", ("job_id",)),
        "observe_provider_health": ("axon_observe_provider_health", ("provider_id",)),
    }
    FILE_TYPES = {"artifact", "auth_session", "chat_session", "evidence", "http_stream",
                  "mcp_session", "operation", "token"}
    def __init__(self, header: Any, manifest_api: Any):
        self.header, self.manifest_api = header, manifest_api; self.round_trips = 0
    def _db(self) -> sqlite3.Connection:
        path = self.header.data_dir / "jobs.db"
        if not path.exists(): raise ProviderError("isolated SQLite state is unavailable")
        try: return sqlite3.connect(f"file:{path}?mode=rw", uri=True, timeout=2)
        except sqlite3.Error as error: raise ProviderError(f"isolated SQLite state is unknown: {error}") from error
    def _path(self, resource: Any) -> Path:
        value = resource.metadata.get("state_file")
        if not isinstance(value, str) or not value: raise ProviderError(f"{resource.resource_type} requires durable state_file evidence")
        path = Path(value).resolve(); root = self.header.data_dir.parent.resolve()
        if root not in path.parents: raise ProviderError("durable state evidence is outside the owned run")
        return path
    def _selector(self, resource: Any) -> tuple[str, tuple[Any, ...]]:
        table, columns = self.TABLES[resource.resource_type]
        if len(columns) == 1: values = (resource.identity,)
        else:
            key = resource.metadata.get("db_key")
            if not isinstance(key, dict) or any(column not in key for column in columns):
                raise ProviderError(f"{resource.resource_type} requires exact composite db_key")
            values = tuple(key[column] for column in columns)
        return f"{' AND '.join(f'{column} = ?' for column in columns)}", values
    def exists(self, resource: Any) -> bool:
        self.round_trips += 1
        if resource.resource_type in self.FILE_TYPES: return self._path(resource).exists()
        table, _ = self.TABLES[resource.resource_type]; where, values = self._selector(resource)
        try:
            with closing(self._db()) as db, db:
                row = db.execute(f"SELECT 1 FROM {table} WHERE {where} LIMIT 1", values).fetchone()
                return row is not None
        except sqlite3.Error as error: raise ProviderError(f"durable state audit failed: {error}") from error
    def marker(self, resource: Any) -> dict[str, Any] | None:
        if not self.exists(resource): return None
        return self.manifest_api.provider_marker(self.header, resource)
    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        if not self.exists(resource): raise ProviderError("cannot bind absent durable state")
        state={"resource_type":resource.resource_type,"identity":resource.identity}
        return self.manifest_api.write_provider_ledger(self.header,resource,state)
    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() >= deadline: raise TimeoutError("durable state deadline exceeded")
        if resource.resource_type in self.FILE_TYPES:
            path = self._path(resource)
            if not path.exists(): return "absent"
            if path.is_dir(): __import__("shutil").rmtree(path)
            else: path.unlink()
            return "removed"
        table, _ = self.TABLES[resource.resource_type]; where, values = self._selector(resource)
        try:
            with closing(self._db()) as db, db:
                if resource.resource_type == "provider_reservation":
                    row = db.execute("SELECT status FROM provider_reservations WHERE reservation_id = ?", (resource.identity,)).fetchone()
                    if row and row[0] in {"queued", "granted", "active"}:
                        db.execute("UPDATE provider_reservations SET status='released', granted_units=0 WHERE reservation_id=?", (resource.identity,))
                cursor = db.execute(f"DELETE FROM {table} WHERE {where}", values)
                return "removed" if cursor.rowcount else "absent"
        except sqlite3.Error as error: raise ProviderError(f"durable state deletion failed: {error}") from error
    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        if not resources: return []
        if resources[0].resource_type in self.FILE_TYPES:
            return [(item, self.delete(item, deadline)) for item in resources]
        table, columns = self.TABLES[resources[0].resource_type]
        if len(columns) != 1: return [(item, self.delete(item, deadline)) for item in resources]
        column = columns[0]
        identities = [item.identity for item in resources]; placeholders = ",".join("?" for _ in identities)
        try:
            self.round_trips += 1
            with closing(self._db()) as db, db:
                if resources[0].resource_type == "provider_reservation":
                    db.execute(f"UPDATE {table} SET status='released', granted_units=0 WHERE {column} IN ({placeholders}) AND status IN ('queued','granted','active')", identities)
                existing = {row[0] for row in db.execute(f"SELECT {column} FROM {table} WHERE {column} IN ({placeholders})", identities)}
                db.execute(f"DELETE FROM {table} WHERE {column} IN ({placeholders})", identities)
            return [(item, "removed" if item.identity in existing else "absent") for item in resources]
        except sqlite3.Error as error: raise ProviderError(f"durable batch deletion failed: {error}") from error
    def batch_capability(self, resource_type: str) -> str:
        return "sqlite-set-batch" if resource_type in self.TABLES else "unbatchable-file-state"

    SECRET_PATTERNS = (
        re.compile(r"(?i)(authorization\s*:\s*bearer\s+)[A-Za-z0-9._~+/=-]+"),
        re.compile(r"(?i)((?:api[_-]?key|token|password|secret)\s*[=:]\s*)[^\s,;]+"),
        re.compile(r"\b(?:sk|ghp|github_pat|xox[baprs])[-_][A-Za-z0-9_-]{12,}\b"),
        re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"),
    )
    def sanitize_evidence(self, resource: Any) -> dict[str, str]:
        source = self._path(resource)
        try: raw = source.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error: raise ProviderError(f"evidence is not readable UTF-8: {error}") from error
        sanitized = raw
        matches = 0
        for pattern in self.SECRET_PATTERNS:
            sanitized, count = pattern.subn(lambda match: (match.group(1) if match.lastindex else "") + "[REDACTED]", sanitized)
            matches += count
        # Verify the produced bytes independently; a caller boolean is never trusted.
        for pattern in self.SECRET_PATTERNS:
            if any("[REDACTED]" not in match.group(0) for match in pattern.finditer(sanitized)):
                raise ProviderError("sanitized evidence still contains a secret pattern")
        retained_root = self.header.data_dir.parent.parent / ".axon-e2e-retained" / self.header.run_id
        retained_root.mkdir(parents=True, exist_ok=True, mode=0o700)
        destination = retained_root / f"{resource.opaque_id}.sanitized"
        destination.write_text(sanitized, encoding="utf-8"); os.chmod(destination, 0o600)
        verified = destination.read_bytes(); digest = hashlib.sha256(verified).hexdigest()
        if hashlib.sha256(sanitized.encode()).hexdigest() != digest: raise ProviderError("sanitized evidence checksum verification failed")
        source.unlink()
        return {"path": str(destination), "sha256": digest, "redactions": str(matches)}


class ArtifactStoreAdapter:
    """Exact cleanup for Axon's public opaque artifact store.

    The JSON manifest is the store authority: it must name the requested
    artifact and its canonical sibling content file before either is removed.
    Ownership remains bound by the signed provider ledger, so an opaque ID
    cannot be adopted merely because it is present under the run directory.
    """
    absent_is_clean = True
    def __init__(self, config: dict[str, Any], header: Any, manifest_api: Any):
        self.header, self.manifest_api = header, manifest_api; self.round_trips = 0
        self.root = Path(config["root"]).resolve()
        if self.header.data_dir.resolve() not in self.root.parents:
            raise ProviderError("artifact store is outside the owned data directory")
    def _paths(self, resource: Any) -> tuple[Path, Path]:
        manifest = self.root / f"{resource.identity}.json"
        content = self.root / f"{resource.identity}.bin"
        return manifest, content
    def _state(self, resource: Any) -> dict[str, Any] | None:
        manifest, content = self._paths(resource); self.round_trips += 1
        if not manifest.exists(): return None
        try: body = json.loads(manifest.read_text())
        except (OSError, json.JSONDecodeError) as error: raise ProviderError(f"artifact manifest is unreadable: {error}") from error
        handle = body.get("handle", {})
        if handle.get("artifact_id") != resource.identity or body.get("content_path") != content.name:
            raise ProviderError("artifact manifest identity/content binding changed")
        if not content.is_file(): raise ProviderError("artifact content state is unknown")
        return {"artifact_id": resource.identity, "content_path": content.name}
    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        state = self._state(resource)
        if state is None: raise ProviderError("cannot bind absent artifact")
        return self.manifest_api.write_provider_ledger(self.header, resource, state)
    def marker(self, resource: Any) -> dict[str, Any] | None:
        state = self._state(resource)
        if state is None: return None
        return self.manifest_api.verify_provider_ledger(self.header, resource, state)
    def delete(self, resource: Any, deadline: float) -> str:
        if time.monotonic() >= deadline: raise TimeoutError("artifact cleanup deadline exceeded")
        manifest, content = self._paths(resource)
        if self._state(resource) is None: return "absent"
        content.unlink(); manifest.unlink(); return "removed"
    def exists(self, resource: Any) -> bool: return self._state(resource) is not None
    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        return [(resource, self.delete(resource, deadline)) for resource in resources]
    @staticmethod
    def batch_capability(_resource_type: str) -> str: return "unbatchable-artifact-store"
    def snapshot_shared(self, owned: set[tuple[str, str]]) -> list[str]:
        excluded={identity for kind,identity in owned if kind=="artifact"}
        return sorted(path.stem for path in self.root.glob("art_*.json") if path.stem not in excluded)

class UploadStoreAdapter:
    """Batch cleanup against Axon's authoritative upload records and index."""
    absent_is_clean=True
    def __init__(self,config,header,manifest_api):
        self.root=Path(config["root"]).resolve();self.header=header;self.manifest_api=manifest_api;self.round_trips=0
        if header.data_dir.resolve() not in self.root.parents:raise ProviderError("upload store outside owned data directory")
    def _record(self,r):return self.root/f"{r.identity}.json"
    def _state(self,r):
        self.round_trips+=1;p=self._record(r)
        if not p.exists():return None
        body=json.loads(p.read_text());value=body.get("upload_id");identity=value.get("0") if isinstance(value,dict) else value
        if identity!=r.identity:raise ProviderError("upload record identity changed")
        return {"upload_id":r.identity}
    def exists(self,r):return self._state(r) is not None
    def provision_ownership(self,r):return self.manifest_api.write_provider_ledger(self.header,r,self._state(r))
    def marker(self,r):
        state=self._state(r);return None if state is None else self.manifest_api.verify_provider_ledger(self.header,r,state)
    def delete(self,r,deadline):return self.delete_batch([r],deadline)[0][1]
    def delete_batch(self,resources,deadline):
        ids={r.identity for r in resources};index=self.root/".upload-index.json"
        for r in resources:
            for suffix in (".json",".part",".lock"):
                p=self.root/f"{r.identity}{suffix}"
                if p.exists():p.unlink()
        if index.exists():
            body=json.loads(index.read_text())
            def keep(item):
                value=item.get("upload_id") if isinstance(item,dict) else None
                if isinstance(value,dict):value=value.get("0")
                return value not in ids
            body["by_id"]=[x for x in body.get("by_id",[]) if keep(x)]
            body["by_expiry"]=[x for x in body.get("by_expiry",[]) if keep(x)]
            body["by_status"]={k:[x for x in values if ((x.get("0") if isinstance(x,dict) else x) not in ids)] for k,values in body.get("by_status",{}).items()}
            tmp=index.with_suffix(".tmp");tmp.write_text(json.dumps(body,sort_keys=True));os.replace(tmp,index)
        return [(r,"removed") for r in resources]
    @staticmethod
    def batch_capability(_kind):return "upload-index-batch"
    def snapshot_shared(self,owned):
        excluded={i for k,i in owned if k=="upload"};return sorted(p.stem for p in self.root.glob("upl_*.json") if p.stem not in excluded)

class ManifestOnlyAdapter:
    """Adapter for manifest relationship records with no independent state."""
    absent_is_clean = True
    round_trips = 0
    def marker(self, _resource): return None
    def exists(self, _resource): self.round_trips += 1; return False
    def delete(self, _resource, _deadline): return "already-absent"
    def snapshot_shared(self, _owned): return []


class FileStateAdapter:
    """Exact state-file adapter for Chrome, Tailscale, and ephemeral credentials."""

    def __init__(self, header: Any, manifest_api: Any): self.header, self.manifest_api = header, manifest_api; self.round_trips = 0
    def _path(self, resource: Any) -> Path:
        path = Path(resource.metadata.get("state_file", resource.identity)).resolve()
        if self.header.data_dir.parent not in path.parents: raise ProviderError("state file is outside owned run")
        return path
    def marker(self, resource: Any) -> dict[str, Any] | None:
        path = self._path(resource); self.round_trips += 1
        if not path.exists(): return None
        try: return json.loads(path.read_text())["ownership"]
        except (OSError, KeyError, json.JSONDecodeError) as error: raise ProviderError("state marker is unknown") from error
    def delete(self, resource: Any, deadline: float) -> str:
        path = self._path(resource); self.round_trips += 1
        if not path.exists(): return "absent"
        path.unlink(); return "removed"
    def exists(self, resource: Any) -> bool: self.round_trips += 1; return self._path(resource).exists()
    def delete_batch(self, resources: list[Any], deadline: float) -> list[tuple[Any, str]]:
        return [(resource, self.delete(resource, deadline)) for resource in resources]


class TailscaleAdapter(FileStateAdapter):
    """Manage only an isolated tailscaled instance through its owned socket."""

    def __init__(self, config: dict[str, Any], header: Any, manifest_api: Any):
        super().__init__(header, manifest_api); self.binary = str(config.get("binary", "tailscale"))
        self.tailscaled_binary = str(config.get("tailscaled_binary", "tailscaled"))
        self.timeout = float(config.get("timeout_seconds", 10))
        self.snapshot_socket = config.get("socket")
        self._managed: dict[str, Any] = {}
    def _socket(self, resource: Any) -> Path:
        value = resource.metadata.get("socket")
        if not isinstance(value, str) or not value: raise ProviderError("Tailscale resource requires an isolated socket")
        path = Path(value).resolve()
        if self.header.data_dir.parent not in path.parents: raise ProviderError("Tailscale socket is outside owned run")
        return path
    def _status(self, resource: Any) -> tuple[subprocess.CompletedProcess[str], dict[str, Any] | None]:
        result = subprocess.run([self.binary, "--socket", str(self._socket(resource)), "status", "--json"],
                                capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
        if result.returncode: return result, None
        try: body = json.loads(result.stdout)
        except json.JSONDecodeError as error: raise ProviderError("tailscale status returned malformed JSON") from error
        return result, body
    def marker(self, resource: Any) -> dict[str, Any] | None:
        result, body = self._status(resource)
        if result.returncode or not isinstance(body, dict) or body.get("BackendState") == "Stopped": return None
        node_id = body.get("Self", {}).get("ID")
        if not isinstance(node_id, str) or not node_id: raise ProviderError("isolated tailscaled lacks stable Self.ID")
        return self.manifest_api.verify_provider_ledger(
            self.header, resource, {"socket": str(self._socket(resource)), "node_id": node_id})
    def provision_ownership(self, resource: Any) -> dict[str, Any]:
        result, body = self._status(resource)
        if result.returncode or not isinstance(body, dict): raise ProviderError("isolated tailscaled is absent")
        node_id = body.get("Self", {}).get("ID")
        if not isinstance(node_id, str) or not node_id: raise ProviderError("isolated tailscaled lacks stable Self.ID")
        return self.manifest_api.write_provider_ledger(
            self.header, resource, {"socket": str(self._socket(resource)), "node_id": node_id})
    def exists(self, resource: Any) -> bool:
        result, body = self._status(resource)
        return result.returncode == 0 and isinstance(body, dict) and body.get("BackendState") != "Stopped"
    def start_and_provision(self, resource: Any) -> dict[str, Any]:
        """Start a per-run daemon and durably bind its immutable socket/node ID."""
        state = self._path(resource); socket_path = self._socket(resource)
        # The signed intent must precede every provider/local allocation so a
        # crash cannot strand an unclaimable daemon, state file, or socket.
        self.manifest_api.write_setup_intent(self.header, resource)
        manifest = self.manifest_api.isolation.Manifest.open(self.header.manifest_path)
        manifest.register("socket", str(socket_path), {"owner": "isolated-tailscaled"})
        manifest.register("temp_path", str(state), {"owner": "isolated-tailscaled-state"})
        managed = self.manifest_api.isolation.spawn_owned_process(
            manifest, self.header.data_dir.parent,
            [self.tailscaled_binary, "--state", str(state), "--socket", str(socket_path)])
        self.header, resources = self.manifest_api.load(self.header.manifest_path)
        resource = next(item for item in resources if
                        (item.resource_type, item.identity) == (resource.resource_type, resource.identity))
        process = managed.process
        self._managed[resource.identity] = managed
        deadline = time.monotonic() + self.timeout
        body = None
        while time.monotonic() < deadline and process.poll() is None:
            try:
                result, body = self._status(resource)
                if result.returncode == 0 and isinstance(body, dict) and body.get("Self", {}).get("ID"): break
            except (OSError, ProviderError): pass
            time.sleep(.05)
        else:
            process.terminate(); process.wait(timeout=2)
            raise ProviderError("isolated tailscaled did not become ready")
        node_id = body["Self"]["ID"]
        proof = self.manifest_api.write_provider_ledger(
            self.header, resource, {"socket": str(socket_path), "node_id": node_id})
        return {"pid": process.pid, "socket": str(socket_path), "state_file": str(state), "marker": proof["marker"]}
    def delete(self, resource: Any, deadline: float) -> str:
        path = self._path(resource)
        if not path.exists(): return "absent"
        self.round_trips += 1
        try:
            result = subprocess.run([self.binary, "--socket", str(self._socket(resource)), "logout"], capture_output=True,
                                    text=True, timeout=min(self.timeout, max(.1, deadline - time.monotonic())),
                                    check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ProviderError(f"Tailscale logout state is unknown: {error}") from error
        if result.returncode: raise ProviderError("Tailscale logout failed")
        managed = self._managed.pop(resource.identity, None)
        if managed is not None and managed.process.poll() is None:
            managed.process.terminate()
            try: managed.process.wait(timeout=min(5, self.timeout))
            except subprocess.TimeoutExpired:
                managed.process.kill(); managed.process.wait(timeout=2)
        path.unlink(missing_ok=True); self._socket(resource).unlink(missing_ok=True); return "removed"
    def recover_creating(self, resource: Any, deadline: float) -> str:
        self.manifest_api.verify_setup_intent(self.header, resource)
        return self.delete(resource, deadline)
    def snapshot_shared(self, owned: set[tuple[str, str]]) -> dict[str, Any]:
        if not self.snapshot_socket:
            raise ProviderError("Tailscale shared checks require a dedicated isolated daemon socket")
        try:
            result = subprocess.run([self.binary, "--socket", str(self.snapshot_socket), "status", "--json"],
                                    capture_output=True, text=True, timeout=self.timeout, check=False, shell=False)
        except (OSError, subprocess.TimeoutExpired) as error: raise ProviderError(f"tailnet snapshot unknown: {error}") from error
        if result.returncode: raise ProviderError("isolated tailscaled snapshot failed")
        try:
            body = json.loads(result.stdout)
            if not isinstance(body, dict): raise ValueError("status is not an object")
            # This socket is the isolation boundary. Mutable login/backend state
            # is expected to change during logout and is deliberately excluded.
            return {"isolated_socket": str(Path(self.snapshot_socket).resolve()), "contract_verified": True}
        except (json.JSONDecodeError, ValueError) as error: raise ProviderError("isolated tailscaled snapshot malformed") from error
