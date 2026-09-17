from __future__ import annotations

import datetime as dt
import importlib.util
import json
import sys
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
GATEWAY = ROOT / "scripts/e2e/gateway"
sys.path.insert(0, str(GATEWAY))
from providers import ProviderAdapter, ProviderFailure
from server import Gateway
from state import LeaseStore, StateError, format_time


class FakeProvider:
    def __init__(self): self.calls=[]; self.fail_cleanup=False
    def proxy(self, method, path, headers, body, lease):
        self.calls.append((method,path,lease["namespace"])); return 200,"application/json",b'{"ok":true}'
    def cleanup(self, lease):
        if self.fail_cleanup: raise ProviderFailure("uncertain")
        return []


class Running:
    def __init__(self, root, clock=None):
        self.provider=FakeProvider();self.store=LeaseStore(Path(root)/"leases.db","qdrant",clock=clock)
        self.server=Gateway(("127.0.0.1",0),service="qdrant",peer="q.ts.net",tag="tag:q",token="secret",
                            store=self.store,provider=self.provider)
        self.thread=threading.Thread(target=self.server.serve_forever,daemon=True);self.thread.start()
        self.url=f"http://127.0.0.1:{self.server.server_port}"
    def close(self): self.server.shutdown();self.server.server_close();self.thread.join()
    def request(self,path,method="GET",body=None,token="secret",api_key=None):
        data=None if body is None else json.dumps(body).encode()
        headers={"Authorization":"Bearer "+token,"Content-Type":"application/json"}
        if api_key is not None:headers["api-key"]=api_key
        req=urllib.request.Request(self.url+path,data=data,method=method,headers=headers)
        try:
            with urllib.request.urlopen(req,timeout=3) as response:return response.status,json.load(response)
        except urllib.error.HTTPError as error:
            try:return error.code,json.load(error)
            finally:error.close()


def lease(now, *, lease_id="axon_e2e_123_1_nonce_qdrant_token", namespace="axon_e2e_123_1_nonce", qps=4):
    return {"lease_id":lease_id,"namespace":namespace,"owner":"dinglebear-ai/axon","run_id":"123",
            "run_attempt":"1","tested_sha":"a"*40,"expires_at":format_time(now+dt.timedelta(seconds=7200)),
            "heartbeat_at":format_time(now),"ttl_seconds":7200,"heartbeat_seconds":30,"max_concurrency":1,"qps":qps}


class GatewayTests(unittest.TestCase):
    def test_auth_is_required_for_management_and_data_plane(self):
        with tempfile.TemporaryDirectory() as root:
            live=Running(root)
            try:
                self.assertEqual(401,live.request("/v1/e2e/identity",token="wrong")[0])
                now=dt.datetime.now(dt.timezone.utc);live.request("/v1/e2e/leases","POST",lease(now))
                self.assertEqual(401,live.request("/collections/x",token="wrong",api_key="wrong")[0])
            finally:live.close()

    def test_ownership_lifecycle_restart_and_single_active_admission(self):
        now=dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)
        with tempfile.TemporaryDirectory() as root:
            live=Running(root,lambda:now)
            try:
                status,created=live.request("/v1/e2e/leases","POST",lease(now));self.assertEqual(201,status)
                self.assertEqual("qdrant",created["provider"])
                self.assertRegex(created["data_token"],r"^axe1_[0-9a-f]{64}$")
                self.assertEqual(401,live.request("/collections/default")[0],"management token cannot access data plane")
                self.assertEqual(200,live.request("/collections/default",api_key=created["data_token"])[0])
                _,visible=live.request("/v1/e2e/leases/"+created["lease_id"])
                self.assertNotIn("data_token",visible)
                self.assertEqual(409,live.request("/v1/e2e/leases","POST",lease(now,lease_id="axon_e2e_other_qdrant_x",namespace="axon_e2e_other"))[0])
                bad={"namespace":created["namespace"],"owner":created["owner"],"run_id":"999","run_attempt":"1"}
                self.assertEqual(409,live.request("/v1/e2e/leases/"+created["lease_id"]+"/heartbeat","PATCH",bad)[0])
            finally:live.close()
            restarted=LeaseStore(Path(root)/"leases.db","qdrant",clock=lambda:now)
            self.assertEqual(created["lease_id"],restarted.get(created["lease_id"])["lease_id"])

    def test_expired_lease_blocks_traffic_and_new_admission_until_reaped(self):
        current=[dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)]
        with tempfile.TemporaryDirectory() as root:
            live=Running(root,lambda:current[0])
            try:
                body=lease(current[0]);_,created=live.request("/v1/e2e/leases","POST",body);current[0]+=dt.timedelta(seconds=7201)
                self.assertEqual(409,live.request("/collections/default",api_key=created["data_token"])[0])
                self.assertEqual(200,live.request("/v1/e2e/leases/"+body["lease_id"])[0],"expired cleanup authority must remain discoverable")
                self.assertEqual(409,live.request("/v1/e2e/leases","POST",lease(current[0],lease_id="axon_e2e_new_qdrant_x",namespace="axon_e2e_new"))[0])
                self.assertEqual((200,{"status":"passed","residuals":[]}),live.request("/v1/e2e/leases/reap","POST",{"owner":"dinglebear-ai/axon","expired_only":True,"residual_audit":True}))
            finally:live.close()

    def test_cleanup_failure_preserves_lease_authority(self):
        now=dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)
        with tempfile.TemporaryDirectory() as root:
            live=Running(root,lambda:now)
            try:
                item=lease(now);live.request("/v1/e2e/leases","POST",item);live.provider.fail_cleanup=True
                delete={"namespace":item["namespace"],"owner":item["owner"],"residual_audit":True}
                self.assertEqual(502,live.request("/v1/e2e/leases/"+item["lease_id"],"DELETE",delete)[0])
                self.assertIsNotNone(live.store.get(item["lease_id"]))
            finally:live.close()

    def test_qdrant_paths_are_rewritten_and_escapes_are_denied(self):
        adapter=ProviderAdapter("qdrant","http://127.0.0.1:1");lease_row={"namespace":"axon_e2e_owned"}
        self.assertEqual("/collections/axon_e2e_owned/points?wait=true",adapter.path("/collections/shared/points?wait=true",lease_row))
        self.assertEqual("/collections/axon_e2e_owned/facet",adapter.path("/collections/shared/facet",lease_row))
        for path in ("/cluster","/collections","/collections/x/snapshots","/collections/x%2fother/points","/collections/../x"):
            if path == "/collections": continue
            with self.assertRaises(ProviderFailure):adapter.path(path,lease_row)
        for body in (b'{"lookup_from":"shared"}',b'{"nested":{"from_collection":"other"}}'):
            with self.assertRaises(ProviderFailure):adapter._validate_body(body,"axon_e2e_owned")

        with self.assertRaisesRegex(ProviderFailure, "facet requires POST"):
            adapter.proxy("GET", "/collections/shared/facet", {}, b"{}", lease_row)

    def test_policy_caps_caller_requested_limits_and_identifiers(self):
        now=dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)
        with tempfile.TemporaryDirectory() as root:
            store=LeaseStore(Path(root)/"leases.db","qdrant",clock=lambda:now)
            for mutation in ({"qps":5},{"ttl_seconds":10},{"heartbeat_seconds":10},{"namespace":"axon_e2e_bad/slash"}):
                body=lease(now);body.update(mutation)
                if "ttl_seconds" in mutation:body["expires_at"]=format_time(now+dt.timedelta(seconds=10))
                with self.assertRaises(StateError):store.create(body)

    def test_heartbeat_deadline_and_immutable_run_expiry(self):
        current=[dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)]
        with tempfile.TemporaryDirectory() as root:
            store=LeaseStore(Path(root)/"leases.db","qdrant",clock=lambda:current[0]);item=store.create(lease(current[0]))
            original=item["expires_at"];current[0]+=dt.timedelta(seconds=30)
            renewed=store.heartbeat(item["lease_id"],{key:item[key] for key in ("namespace","owner","run_id","run_attempt")})
            self.assertEqual(original,renewed["expires_at"])
            current[0]+=dt.timedelta(seconds=91)
            with self.assertRaises(StateError):store.heartbeat(item["lease_id"],{key:item[key] for key in ("namespace","owner","run_id","run_attempt")})

    def test_sqlite_admission_is_atomic_across_store_instances(self):
        now=dt.datetime(2026,9,13,tzinfo=dt.timezone.utc)
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/"leases.db";stores=[LeaseStore(path,"qdrant",clock=lambda:now) for _ in range(2)]
            barrier=threading.Barrier(2);outcomes=[]
            def create(index):
                barrier.wait()
                try:
                    stores[index].create(lease(now,lease_id=f"axon_e2e_run{index}_qdrant_x",namespace=f"axon_e2e_run{index}"));outcomes.append("created")
                except StateError:outcomes.append("refused")
            threads=[threading.Thread(target=create,args=(index,)) for index in range(2)]
            for thread in threads:thread.start()
            for thread in threads:thread.join()
            self.assertEqual(["created","refused"],sorted(outcomes))


if __name__ == "__main__": unittest.main()
