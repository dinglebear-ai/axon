from __future__ import annotations
import dataclasses,importlib.util,json,os,signal,sys,tempfile,threading,time,unittest
import stat
from http.server import ThreadingHTTPServer
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3];LIB=ROOT/"scripts/e2e/lib";FIX=ROOT/"tests/e2e/fixtures/teardown"
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);sys.modules[name]=module;spec.loader.exec_module(module);return module
teardown=load("contract_teardown",LIB/"teardown.py"); qcontract=load("contract_qdrant",FIX/"qdrant_contract.py")
fake_module=load("contract_fake_other",FIX/"fake_provider.py"); isolation=teardown.isolation

class ProviderContractIntegrationTests(unittest.TestCase):
    def test_assertion_death_with_provider_residue_before_manifest_persistence_is_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");run=allocation["run_id"]
            orphan=f"{run}_created_before_manifest_append";script=FIX/"provider_contract_cli.py";script.chmod(script.stat().st_mode|stat.S_IXUSR)
            bindir=root/"bin";bindir.mkdir();(bindir/"docker").symlink_to(script)
            state_path=root/"providers.json";state_path.write_text(json.dumps({"fail_next":None,"docker":{"container":{},"network":{},
                "volume":{orphan:{"Name":orphan,"Labels":{"axon.e2e.ownership":"unpersisted"}},"shared-volume":{"Name":"shared-volume","Labels":{"operator":"true"}}}},
                "compose":{},"watch":{},"uploads":{},"tailscale":{"Self":{},"Peer":{}}}))
            old=os.environ.get("AXON_E2E_PROVIDER_STATE");os.environ["AXON_E2E_PROVIDER_STATE"]=str(state_path)
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));docker=teardown.provider_api.DockerAdapter({"binary":str(bindir/"docker"),"resources":{
                    "network":{"inspect":["network","inspect","{identity}"],"delete":["network","rm","{identity}"]},
                    "volume":{"inspect":["volume","inspect","{identity}"],"delete":["volume","rm","{identity}"]}}},header,teardown.manifest_api)
                fake=fake_module.FakeProvider(teardown.manifest_api,header,resources);adapters={kind:fake for kind in teardown.PROVIDER_TYPES};adapters.update({"network":docker,"volume":docker})
                report=teardown.Engine(Path(allocation["manifest"]),adapters).run().json();self.assertFalse(report["success"])
                self.assertTrue(any("before manifest/provider-ledger persistence" in item["reason"] for item in report["refused"]))
                remaining=json.loads(state_path.read_text())["docker"]["volume"];self.assertIn(orphan,remaining);self.assertIn("shared-volume",remaining)
            finally:
                if old is None:os.environ.pop("AXON_E2E_PROVIDER_STATE",None)
                else:os.environ["AXON_E2E_PROVIDER_STATE"]=old

    def test_qdrant_create_config_marker_upsert_and_readback_failures_are_executable(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));run=allocation["run_id"]
            collection=next(r for r in resources if r.resource_type=="collection");state_path=root/"qdrant.json";qcontract.Handler.state_path=state_path
            server=ThreadingHTTPServer(("127.0.0.1",0),qcontract.Handler);thread=threading.Thread(target=server.serve_forever);thread.start()
            try:
                adapter=teardown.provider_api.QdrantAdapter({"base_url":f"http://127.0.0.1:{server.server_port}","tenant_enforced":True,"owned_prefix":"axon_e2e_"}).bind(header,teardown.manifest_api)
                for token,stage in ((f"GET /collections/{run}","collection-config"),(f"PUT /collections/{run}/points","marker-upsert"),(f"POST /collections/{run}/points","marker-readback")):
                    state_path.write_text(json.dumps({"fail_next":token,"aliases":{},"collections":{run:{"size":3,"indexes":{},"snapshots":{},"points":{}}}}))
                    with self.subTest(stage=stage):
                        with self.assertRaises(Exception):adapter.provision_ownership_marker(collection)
            finally:server.shutdown();thread.join();server.server_close()

    def test_failure_after_every_file_backed_provider_setup_stage_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            manifest.register("container",f"{run}_container",{"ownership_generation":os.urandom(32).hex(),"image":"fixture@sha256:abc"})
            manifest.register("volume",f"{run}_volume",{"ownership_generation":os.urandom(32).hex()})
            manifest.register("watch",f"{run}_watch",{"ownership_generation":os.urandom(32).hex()});manifest.register("upload",f"{run}_upload",{"ownership_generation":os.urandom(32).hex()})
            state_file=Path(allocation["data_dir"]).parent/"tailscale.state";manifest.register("tailscale_node",f"{run}_node",{"state_file":str(state_file),"socket":str(state_file.with_suffix(".sock")),"ownership_generation":os.urandom(32).hex()})
            header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));by={(r.resource_type,r.identity):r for r in resources}
            script=FIX/"provider_contract_cli.py";script.chmod(script.stat().st_mode|stat.S_IXUSR);bindir=root/"bin";bindir.mkdir()
            for name in ("docker","tailscale","axon"):(bindir/name).symlink_to(script)
            state_path=root/"providers.json";base={"fail_next":None,"docker":{"container":{},"network":{},"volume":{}},
                "compose":{run:{"Name":run,"Status":"running","ConfigFiles":"owned.yaml"}},
                "watch":{f"{run}_watch":{"watch_id":f"{run}_watch"}},"uploads":{f"{run}_upload":{"upload_id":f"{run}_upload"}},
                "tailscale":{"Self":{"ID":"operator"},"Peer":{}}};state_path.write_text(json.dumps(base));old=os.environ.get("AXON_E2E_PROVIDER_STATE");os.environ["AXON_E2E_PROVIDER_STATE"]=str(state_path)
            try:
                docker=teardown.provider_api.DockerAdapter({"binary":str(bindir/"docker"),"resources":{kind:{"inspect":[kind,"inspect","{identity}"],"delete":[kind,"rm","{identity}"]} for kind in ("container","network","volume")}},header,teardown.manifest_api)
                for kind,identity in (("container",f"{run}_container"),("network",run),("volume",f"{run}_volume")):
                    current=json.loads(json.dumps(base));current["fail_next"]=f"docker:{kind}:create";state_path.write_text(json.dumps(current))
                    with self.subTest(stage=f"docker-{kind}-create"):
                        with self.assertRaisesRegex(Exception,"creation failed"):docker.provision_ownership(by[(kind,identity)])
                state_path.write_text(json.dumps(base));compose=teardown.provider_api.DockerComposeAdapter({"binary":str(bindir/"docker")},header,teardown.manifest_api)
                current=json.loads(json.dumps(base));current["fail_next"]="docker:compose:-p";state_path.write_text(json.dumps(current))
                with self.assertRaisesRegex(Exception,"cannot bind absent Compose project"):compose.provision_ownership(by[("compose_project",run)])
                axon=teardown.provider_api.ManifestBoundArgvAdapter({"binary":str(bindir/"axon"),"identity_fields":{"watch":["watch_id"],"upload":["upload_id"]},"resources":{
                    "watch":{"inspect":["--json","watch","get","{identity}"],"delete":["--json","watch","delete","{identity}"]},
                    "upload":{"inspect":["--json","uploads","get","{identity}"],"delete":["--json","uploads","abort","{identity}"]}}},header,teardown.manifest_api)
                for kind,family in (("watch","watch"),("upload","uploads")):
                    current=json.loads(json.dumps(base));current["fail_next"]=f"axon:--json:{family}";state_path.write_text(json.dumps(current))
                    with self.subTest(stage=f"axon-{kind}-inspect"):
                        with self.assertRaisesRegex(Exception,"absent provider resource"):axon.provision_ownership(by[(kind,f"{run}_{kind}")])
                state_path.write_text(json.dumps(base));state_file.write_text(json.dumps({"ownership":teardown.manifest_api.provider_marker(header,by[("tailscale_node",f"{run}_node")])}))
                current=json.loads(json.dumps(base));current["fail_next"]="tailscale:--socket:"+str(state_file.with_suffix(".sock").resolve());state_path.write_text(json.dumps(current))
                tailscale=teardown.provider_api.TailscaleAdapter({"binary":str(bindir/"tailscale"),"socket":str(state_file.with_suffix(".sock"))},header,teardown.manifest_api)
                with self.assertRaisesRegex(Exception,"logout failed"):tailscale.delete(by[("tailscale_node",f"{run}_node")],float("inf"))
            finally:
                if old is None:os.environ.pop("AXON_E2E_PROVIDER_STATE",None)
                else:os.environ["AXON_E2E_PROVIDER_STATE"]=old

    def test_all_qdrant_resource_recycled_ids_are_rejected_via_live_state(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            meta=lambda:{"collection":run,"ownership_generation":os.urandom(32).hex()}
            manifest.register("qdrant_alias",f"{run}_alias",meta());manifest.register("qdrant_snapshot",f"{run}_snap",meta())
            manifest.register("payload_index","field",meta());manifest.register("point","point",meta())
            state_path=root/"qdrant.json";state_path.write_text(json.dumps({"fail_next":None,"aliases":{f"{run}_alias":run},"collections":{
                run:{"size":3,"indexes":{"field":{"data_type":"keyword"}},"snapshots":{f"{run}_snap":{"name":f"{run}_snap","checksum":"one"}},"points":{"point":{"id":"point","payload":{"version":"one"}}}}}}))
            qcontract.Handler.state_path=state_path;server=ThreadingHTTPServer(("127.0.0.1",0),qcontract.Handler);thread=threading.Thread(target=server.serve_forever);thread.start()
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));adapter=teardown.provider_api.QdrantAdapter({"base_url":f"http://127.0.0.1:{server.server_port}","tenant_enforced":True,"owned_prefix":"axon_e2e_"}).bind(header,teardown.manifest_api)
                qresources=[r for r in resources if r.resource_type in {"collection","qdrant_alias","qdrant_snapshot","payload_index","point"}]
                for resource in qresources:adapter.provision_ownership_marker(resource)
                for resource in qresources:
                    state=json.loads(state_path.read_text());original=json.loads(json.dumps(state))
                    if resource.resource_type=="collection":state["collections"][run]["size"]=4
                    elif resource.resource_type=="qdrant_alias":state["aliases"][resource.identity]="recycled-target"
                    elif resource.resource_type=="qdrant_snapshot":state["collections"][run]["snapshots"][resource.identity]["checksum"]="two"
                    elif resource.resource_type=="payload_index":state["collections"][run]["indexes"][resource.identity]={"data_type":"integer"}
                    else:state["collections"][run]["points"][resource.identity]["payload"]["version"]="two"
                    state_path.write_text(json.dumps(state))
                    with self.subTest(resource_type=resource.resource_type, mutation="legitimate-lifecycle"):
                        self.assertIsNotNone(adapter.marker(resource))
                    # Reuse is proved by changing the immutable generation marker,
                    # never by hashing mutable optimizer/status/provider output.
                    marker_state=json.loads(state_path.read_text())
                    for point in marker_state["collections"][run]["points"].values():
                        ownership=point.get("payload",{}).get("axon_e2e_ownership",{})
                        if ownership.get("resource_type")==resource.resource_type and ownership.get("identity")==resource.identity:
                            ownership["generation"]="recycled-generation"
                    state_path.write_text(json.dumps(marker_state))
                    with self.subTest(resource_type=resource.resource_type, mutation="recycled-generation"):
                        with self.assertRaisesRegex(Exception,"generation changed"): adapter.marker(resource)
                    state_path.write_text(json.dumps(original))
            finally:server.shutdown();thread.join();server.server_close()

    def test_faithful_provider_failure_and_preledger_death_matrix_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            manifest.register("watch",f"{run}_watch",{"ownership_generation":os.urandom(32).hex()})
            state_file=Path(allocation["data_dir"]).parent/"tailscale.state";manifest.register("tailscale_node",f"{run}_node",{"state_file":str(state_file),"socket":str(state_file.with_suffix(".sock")),"ownership_generation":os.urandom(32).hex()})
            header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));by={(r.resource_type,r.identity):r for r in resources}
            script=FIX/"provider_contract_cli.py";script.chmod(script.stat().st_mode|stat.S_IXUSR);bindir=root/"bin";bindir.mkdir()
            for name in ("docker","tailscale","axon"):(bindir/name).symlink_to(script)
            state_path=root/"providers.json";base={"fail_next":None,"docker":{"container":{},"network":{},"volume":{}},"compose":{},
                "watch":{f"{run}_watch":{"watch_id":f"{run}_watch","version":"pre-ledger"}},"uploads":{},
                "tailscale":{"Self":{"ID":"operator"},"Peer":{}}};state_path.write_text(json.dumps(base));old=os.environ.get("AXON_E2E_PROVIDER_STATE");os.environ["AXON_E2E_PROVIDER_STATE"]=str(state_path)
            try:
                docker=teardown.provider_api.DockerAdapter({"binary":str(bindir/"docker"),"resources":{"network":{"inspect":["network","inspect","{identity}"],"delete":["network","rm","{identity}"]}}},header,teardown.manifest_api)
                current=json.loads(state_path.read_text());current["fail_next"]="docker";state_path.write_text(json.dumps(current))
                with self.assertRaisesRegex(Exception,"creation failed"):docker.provision_ownership(by[("network",run)])
                axon=teardown.provider_api.ManifestBoundArgvAdapter({"binary":str(bindir/"axon"),"identity_fields":{"watch":["watch_id"],"upload":["upload_id"]},"resources":{"watch":{"inspect":["--json","watch","get","{identity}"],"delete":["--json","watch","delete","{identity}"]}}},header,teardown.manifest_api)
                with self.assertRaisesRegex(Exception,"ledger identity is missing or ambiguous"):axon.marker(by[("watch",f"{run}_watch")])
                state_file.write_text(json.dumps({"ownership":teardown.manifest_api.provider_marker(header,by[("tailscale_node",f"{run}_node")])}))
                current=json.loads(state_path.read_text());current["fail_next"]="tailscale";state_path.write_text(json.dumps(current))
                tailscale=teardown.provider_api.TailscaleAdapter({"binary":str(bindir/"tailscale"),"socket":str(state_file.with_suffix(".sock"))},header,teardown.manifest_api)
                with self.assertRaisesRegex(Exception,"logout failed"):tailscale.delete(by[("tailscale_node",f"{run}_node")],float("inf"))
                self.assertTrue(state_file.exists())
            finally:
                if old is None:os.environ.pop("AXON_E2E_PROVIDER_STATE",None)
                else:os.environ["AXON_E2E_PROVIDER_STATE"]=old

    def test_file_backed_real_cli_shapes_cleanup_and_operator_invariance(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            manifest.register("watch",f"{run}_watch",{"ownership_generation":os.urandom(32).hex()})
            manifest.register("upload",f"{run}_upload",{"ownership_generation":os.urandom(32).hex()})
            manifest.register("volume",f"{run}_volume",{"ownership_generation":os.urandom(32).hex()})
            tailscale_state=Path(allocation["data_dir"]).parent/"tailscale.state"
            manifest.register("tailscale_node",f"{run}_node",{"state_file":str(tailscale_state),"socket":str(tailscale_state.with_suffix(".sock")),"ownership_generation":os.urandom(32).hex()})
            state_path=root/"providers.json";state={"fail_next":None,"docker":{"container":{"shared-container":{"Id":"shared-container","Config":{"Labels":{"operator":"true"}}}},
                "network":{"shared-network":{"Id":"shared-network","Labels":{"operator":"true"}}},"volume":{"shared-volume":{"Name":"shared-volume","Labels":{"operator":"true"}}}},
                "compose":{"shared-project":{"ID":"shared-container","Name":"shared-service","Project":"shared-project","Status":"running","ConfigFiles":"shared.yaml"},run:{"ID":f"{run}-container","Name":"owned-service","Project":run,"Status":"running","ConfigFiles":"owned.yaml"}},
                "watch":{"shared-watch":{"watch_id":"shared-watch","owner":"operator"},f"{run}_watch":{"watch_id":f"{run}_watch","version":"one"}},
                "uploads":{"shared-upload":{"upload_id":"shared-upload","owner":"operator"},f"{run}_upload":{"upload_id":f"{run}_upload","version":"one"}},
                "tailscale":{"Self":{"ID":"operator-self"},"Peer":{"shared":{"ID":"shared-node","DNSName":"shared.tailnet"}}}}
            state_path.write_text(json.dumps(state));script=FIX/"provider_contract_cli.py";script.chmod(script.stat().st_mode|stat.S_IXUSR)
            bindir=root/"bin";bindir.mkdir()
            for name in ("docker","tailscale","axon"): (bindir/name).symlink_to(script)
            old=os.environ.get("AXON_E2E_PROVIDER_STATE");os.environ["AXON_E2E_PROVIDER_STATE"]=str(state_path)
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));by={(r.resource_type,r.identity):r for r in resources}
                tailscale_resource=by[("tailscale_node",f"{run}_node")];tailscale_state.write_text(json.dumps({"ownership":teardown.manifest_api.provider_marker(header,tailscale_resource)}))
                docker=teardown.provider_api.DockerAdapter({"binary":str(bindir/"docker"),"resources":{
                    "network":{"inspect":["network","inspect","{identity}"],"delete":["network","rm","{identity}"]},
                    "volume":{"inspect":["volume","inspect","{identity}"],"delete":["volume","rm","{identity}"]}}},header,teardown.manifest_api)
                docker.provision_ownership(by[("network",run)]);docker.provision_ownership(by[("volume",f"{run}_volume")])
                compose=teardown.provider_api.DockerComposeAdapter({"binary":str(bindir/"docker")},header,teardown.manifest_api);compose.provision_ownership(by[("compose_project",run)])
                axon=teardown.provider_api.ManifestBoundArgvAdapter({"binary":str(bindir/"axon"),"identity_fields":{"watch":["watch_id"],"upload":["upload_id"]},"resources":{
                    "watch":{"inspect":["--json","watch","get","{identity}"],"delete":["--json","watch","delete","{identity}"]},
                    "upload":{"inspect":["--json","uploads","get","{identity}"],"delete":["--json","uploads","abort","{identity}"]}}},header,teardown.manifest_api)
                axon.provision_ownership(by[("watch",f"{run}_watch")]);axon.provision_ownership(by[("upload",f"{run}_upload")])
                before_recycle=json.loads(state_path.read_text());recycled=json.loads(json.dumps(before_recycle))
                recycled["uploads"][f"{run}_upload"]["version"]="completed";state_path.write_text(json.dumps(recycled))
                self.assertIsNotNone(axon.marker(by[("upload",f"{run}_upload")]))
                recycled["uploads"][f"{run}_upload"]["upload_id"]="recycled-provider-id";state_path.write_text(json.dumps(recycled))
                with self.assertRaisesRegex(Exception,"recycled or mutated"):
                    axon.marker(by[("upload",f"{run}_upload")])
                state_path.write_text(json.dumps(before_recycle))
                tailscale=teardown.provider_api.TailscaleAdapter({"binary":str(bindir/"tailscale"),"socket":str(tailscale_state.with_suffix(".sock"))},header,teardown.manifest_api)
                tailscale.provision_ownership(tailscale_resource)
                fake=fake_module.FakeProvider(teardown.manifest_api,header,resources);adapters={kind:fake for kind in teardown.PROVIDER_TYPES}
                adapters.update({"network":docker,"volume":docker,"compose_project":compose,"watch":axon,"upload":axon,"tailscale_node":tailscale})
                report=teardown.Engine(Path(allocation["manifest"]),adapters).run().json();self.assertTrue(report["success"],report)
                self.assertTrue(all(item["unchanged"] for item in report["invariants"]));remaining=json.loads(state_path.read_text())
                self.assertIn("shared-network",remaining["docker"]["network"]);self.assertIn("shared-volume",remaining["docker"]["volume"])
                self.assertIn("shared-project",remaining["compose"]);self.assertIn("shared-watch",remaining["watch"]);self.assertIn("shared-upload",remaining["uploads"])
            finally:
                if old is None:os.environ.pop("AXON_E2E_PROVIDER_STATE",None)
                else:os.environ["AXON_E2E_PROVIDER_STATE"]=old

    def test_real_loopback_qdrant_all_resource_markers_cleanup_and_shared_invariance(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            metadata=lambda: {"collection":run,"ownership_generation":os.urandom(32).hex()}
            manifest.register("qdrant_alias",f"{run}_alias",metadata());manifest.register("qdrant_snapshot",f"{run}_snapshot",metadata())
            manifest.register("payload_index","owned_field",metadata());manifest.register("point","owned-point",metadata())
            late_collection=f"{run}_late";late_alias=f"{run}_alias_before_collection"
            manifest.register("qdrant_alias",late_alias,{"collection":late_collection,"ownership_generation":os.urandom(32).hex()})
            manifest.register("collection",late_collection,{"ownership_generation":os.urandom(32).hex()})
            shared_points={f"shared-{index:03d}":{"id":f"shared-{index:03d}","vector":[float(index),0.5,1.0],
                           "payload":{"operator":True,"ordinal":index}} for index in range(300)}
            state_path=root/"qdrant.json";state_path.write_text(json.dumps({"aliases":{f"{run}_alias":run,late_alias:late_collection,"shared-alias":"shared"},"collections":{
                run:{"size":3,"indexes":{"owned_field":{"data_type":"keyword"}},"snapshots":{f"{run}_snapshot":{"name":f"{run}_snapshot","size":42,"checksum":"owned"}},"points":{"owned-point":{"id":"owned-point","payload":{"text":"owned"}}}},
                late_collection:{"size":3,"indexes":{},"snapshots":{},"points":{}},
                "shared":{"size":3,"indexes":{"operator_tag":{"data_type":"keyword"}},
                          "snapshots":{"shared.snap":{"name":"shared.snap","size":999,"checksum":"operator-checksum"}},
                          "points":shared_points}}}))
            qcontract.Handler.state_path=state_path;server=ThreadingHTTPServer(("127.0.0.1",0),qcontract.Handler);thread=threading.Thread(target=server.serve_forever);thread.start()
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));qdrant=teardown.provider_api.QdrantAdapter({"base_url":f"http://127.0.0.1:{server.server_port}","tenant_enforced":True,"owned_prefix":"axon_e2e_"}).bind(header,teardown.manifest_api)
                for resource in resources:
                    if resource.resource_type in {"collection","qdrant_alias","qdrant_snapshot","payload_index","point"}: qdrant.provision_ownership_marker(resource)
                fake=fake_module.FakeProvider(teardown.manifest_api,header,resources);adapters={kind:fake for kind in teardown.PROVIDER_TYPES}
                for kind in ("collection","qdrant_alias","qdrant_snapshot","payload_index","point"):adapters[kind]=qdrant
                report=teardown.Engine(Path(allocation["manifest"]),adapters).run().json();self.assertTrue(report["success"],report)
                self.assertTrue(all(item["unchanged"] for item in report["invariants"]));remaining=json.loads(state_path.read_text())
                self.assertEqual(["shared"],sorted(remaining["collections"]));self.assertEqual({"shared-alias":"shared"},remaining["aliases"])
                qdrant_invariant=next(item for item in report["invariants"] if item["adapter"]=="QdrantAdapter")
                self.assertEqual(qdrant_invariant["before_sha256"],qdrant_invariant["after_sha256"])
                self.assertEqual(300,len(remaining["collections"]["shared"]["points"]))
            finally:server.shutdown();thread.join();server.server_close()

    def test_qdrant_supervised_create_persists_intent_and_marks_every_resource_class(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");run=allocation["run_id"]
            manifest=isolation.Manifest.open(Path(allocation["manifest"]));generation=lambda:os.urandom(32).hex()
            manifest.register("qdrant_alias",f"{run}_alias",{"collection":run,"ownership_generation":generation()})
            manifest.register("qdrant_snapshot",f"{run}_snapshot",{"collection":run,"ownership_generation":generation()})
            manifest.register("point",f"{run}_point",{"collection":run,"ownership_generation":generation()})
            manifest.register("payload_index",f"{run}_field",{"collection":run,"ownership_generation":generation()})
            state_path=root/"qdrant.json";state_path.write_text(json.dumps({"fail_next":None,"aliases":{},"collections":{},"next_snapshot_name":f"{run}_snapshot"}))
            qcontract.Handler.state_path=state_path;server=ThreadingHTTPServer(("127.0.0.1",0),qcontract.Handler)
            thread=threading.Thread(target=server.serve_forever);thread.start()
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));by={r.resource_type:r for r in resources if r.resource_type in {"collection","qdrant_alias","qdrant_snapshot","point","payload_index"}}
                by["collection"]=dataclasses.replace(by["collection"],metadata={**by["collection"].metadata,"create_payload":{"vectors":{"size":3,"distance":"Cosine"}}})
                by["point"]=dataclasses.replace(by["point"],metadata={**by["point"].metadata,"point":{"id":by["point"].identity,"vector":[1.0,0.0,0.0],"payload":{"state":"created"}}})
                by["payload_index"]=dataclasses.replace(by["payload_index"],metadata={**by["payload_index"].metadata,"field_schema":{"field_schema":"keyword"}})
                adapter=teardown.provider_api.QdrantAdapter({"base_url":f"http://127.0.0.1:{server.server_port}","tenant_enforced":True,"owned_prefix":"axon_e2e_"}).bind(header,teardown.manifest_api)
                for kind in ("collection","qdrant_alias","qdrant_snapshot","point","payload_index"):
                    with self.subTest(resource_type=kind):
                        proof=adapter.create_and_provision(by[kind]);self.assertEqual(run,proof["collection"])
                        self.assertIsNotNone(adapter.marker(by[kind]))
                state=json.loads(state_path.read_text());self.assertIn(run,state["collections"]);self.assertEqual(run,state["aliases"][f"{run}_alias"])
            finally:server.shutdown();thread.join();server.server_close()

    def test_tailscale_supervised_daemon_uses_owned_state_socket_and_registers_process(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);allocation=isolation.allocate(root/"runs",root/"manifests");manifest=isolation.Manifest.open(Path(allocation["manifest"]));run=allocation["run_id"]
            state_file=Path(allocation["data_dir"]).parent/"tailscaled.state";socket_file=state_file.with_suffix(".sock")
            manifest.register("tailscale_node",f"{run}_node",{"state_file":str(state_file),"socket":str(socket_file),"ownership_generation":os.urandom(32).hex()})
            script=FIX/"provider_contract_cli.py";script.chmod(script.stat().st_mode|stat.S_IXUSR);bindir=root/"bin";bindir.mkdir()
            for name in ("tailscale","tailscaled"):(bindir/name).symlink_to(script)
            provider_state=root/"providers.json";provider_state.write_text(json.dumps({"fail_next":None,"tailscale":{"BackendState":"Running","Self":{"ID":f"{run}_node"},"Peer":{}},"docker":{},"compose":{},"watch":{},"uploads":{}}))
            old=os.environ.get("AXON_E2E_PROVIDER_STATE");os.environ["AXON_E2E_PROVIDER_STATE"]=str(provider_state)
            pid=None
            try:
                header,resources=teardown.manifest_api.load(Path(allocation["manifest"]));resource=next(r for r in resources if r.resource_type=="tailscale_node")
                adapter=teardown.provider_api.TailscaleAdapter({"binary":str(bindir/"tailscale"),"tailscaled_binary":str(bindir/"tailscaled"),"socket":str(socket_file)},header,teardown.manifest_api)
                proof=adapter.start_and_provision(resource);pid=proof["pid"];self.assertTrue(socket_file.exists());self.assertIsNotNone(adapter.marker(resource))
                records=manifest.verify();self.assertTrue(any(r["payload"].get("resource_type")=="process" and r["payload"].get("identity")==str(pid) for r in records))
                self.assertEqual("removed",adapter.delete(resource,time.monotonic()+5))
            finally:
                if pid:
                    try:os.kill(pid,signal.SIGTERM)
                    except ProcessLookupError:pass
                if old is None:os.environ.pop("AXON_E2E_PROVIDER_STATE",None)
                else:os.environ["AXON_E2E_PROVIDER_STATE"]=old

if __name__=="__main__":unittest.main()
