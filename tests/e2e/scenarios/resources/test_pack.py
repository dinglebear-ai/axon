import importlib.util, unittest
from pathlib import Path

PATH = Path(__file__).with_name("pack.py")
SPEC = importlib.util.spec_from_file_location("resource_pack", PATH); pack = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(pack)

class ResourcePackTests(unittest.TestCase):
    def test_upload_lifecycle_race_inventory_and_growth(self):
        uploads = {}; artifacts = {}; serial = [0]
        def http(method, path, body):
            if method == "POST" and path == "/v1/uploads":
                serial[0] += 1; identity = f"upl_{serial[0]}"; uploads[identity] = {"status":"pending"}; return {"upload_id":identity}
            identity = path.split("/")[3]
            if method == "PUT": uploads[identity]["content"] = bytes(body); return {}
            if path.endswith("/complete"):
                artifact = uploads[identity].setdefault("artifact_id", f"art_{identity}"); uploads[identity]["status"]="completed"; artifacts[artifact]=uploads[identity].get("content", b""); return {"upload_id":identity,"artifact_id":artifact,"source_ref":f"upload:{identity}"}
            if method == "DELETE": uploads[identity]["status"]="aborted"; return {}
            if path.startswith("/v1/artifacts/"):
                artifact = identity
                return artifacts[artifact] if path.endswith("/content") else {"artifact_id":artifact,"size_bytes":len(artifacts[artifact]),"content_type":"text/markdown"}
            return {"upload_id":identity,"sha256":"eafcc374fbfbdf07d51716d7021d8d9889eb11905d89708d8612596d4c2c3a4e", **uploads[identity]}
        registered=[]
        result=pack.upload_artifact_lifecycle(http,"run_x",lambda k,i:registered.append((k,i)))
        self.assertEqual("art_upl_1",result["artifact_id"])
        self.assertIn(pack.upload_complete_abort_race(http,"run_x",lambda k,i:registered.append((k,i)))["status"],{"completed","aborted"})
        self.assertEqual(257,len(pack.register_growth(http,"run_x",lambda k,i:registered.append((k,i)))))
        self.assertEqual(261,len(registered))

    def test_inventory_uses_production_cli_shapes(self):
        calls=[]
        def cli(*args):
            calls.append(args); op=args[0]
            return {"collections":{"collections":[]},"capabilities":{"schema_version":"client-server.v1","minimum_client_schema_version":"client-server.v1","supported_routes":["GET /v1/capabilities"],"build":{},"version":"1"},
                    "providers":{"providers":[{"id":"tei","ok":True,"detail":{}}]},
                    "status":{"build_identity":{},"cleanup":{"jobs":[]},"jobs":[],"sqlite":{"ok":True},"totals":{},"degraded":False,"warnings":[],"watches":[]},
                    "stats":{"collection":"run","status":"green","points_count":0,"payload_fields":[],"freshness":{},"counts":{}},"config":{"server":{}}}[op]
        result=pack.read_only_inventory(cli)
        self.assertEqual(6,len(result)); self.assertIn(("collections","list","--json"),calls)

if __name__ == "__main__": unittest.main()
