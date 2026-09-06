import importlib.util, json, os, stat, tempfile, unittest
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading

PATH=Path(__file__).with_name("run.py"); SPEC=importlib.util.spec_from_file_location("stateful_run",PATH); run=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(run)

class RunnerTransportTests(unittest.TestCase):
 def test_launcher_requires_exact_per_run_collection_proof(self):
  with tempfile.TemporaryDirectory() as directory:
   binary=Path(directory)/"axon"; binary.write_bytes(b"binary"); descriptor=Path(directory)/"descriptor.json"
   launcher=Path(directory)/"launcher"; payload={"schema":1,"run_id":"run_1","status":"running","http_base_url":"http://127.0.0.1:8123","mcp_selector":"axon.axon","qdrant_url":"http://127.0.0.1:6333","environment":{"AXON_COLLECTION":"run_1","AXON_MEMORY_COLLECTION":"run_1"},"collection_marker":{"collection":"run_1","run_id":"run_1","marker_id":"snapshot_run_1","ownership_generation":"gen_1"},"binary":str(binary),"binary_sha256":__import__('hashlib').sha256(b"binary").hexdigest(),"process_ids":{"axon":123},"descriptor_path":str(descriptor),"teardown_handle":{"command":["/usr/bin/true"]}}
   launcher.write_text(f"#!/bin/sh\nread ignored\nprintf '%s\\n' '{json.dumps(payload)}'\n"); launcher.chmod(0o700)
   allocation={"run_id":"run_1","collection":"run_1","ownership_generation":"gen_1"}
   self.assertEqual("run_1",run.launch_runtime(launcher,allocation,2)["environment"]["AXON_MEMORY_COLLECTION"])
   payload["environment"]["AXON_MEMORY_COLLECTION"]="shared"
   launcher.write_text(f"#!/bin/sh\nread ignored\nprintf '%s\\n' '{json.dumps(payload)}'\n")
   with self.assertRaisesRegex(run.RunnerError,r"source\+memory collections"): run.launch_runtime(launcher,allocation,2)
 def test_operation_specific_contracts_reject_missing_fields(self):
  self.assertEqual({"all_ok":True},run.require_fields({"all_ok":True},("all_ok",),"doctor"))
  with self.assertRaisesRegex(run.RunnerError,"omitted all_ok"):
   run.require_fields({"items":[]},("all_ok",),"doctor")
 def test_identity_set_is_exact_and_recursive(self):
  self.assertEqual({"upl_a","upl_b"},run.identity_set({"items":[{"upload_id":"upl_a"},{"nested":{"upload_id":"upl_b"}}]},("upload_id",)))
 def test_cli_process_rejects_schema_drift(self):
  with tempfile.TemporaryDirectory() as directory:
   binary=Path(directory)/"axon"; binary.write_text("#!/bin/sh\nprintf '{\"items\":[]}\\n'\n"); binary.chmod(binary.stat().st_mode|stat.S_IXUSR)
   self.assertEqual([],run.Cli(binary,{},2)("collections","list","--json")["items"])
   binary.write_text("#!/bin/sh\nprintf '[]\\n'\n")
   with self.assertRaisesRegex(run.RunnerError,"structured JSON"): run.Cli(binary,{},2)("collections","list","--json")
 def test_mcporter_process_decodes_production_content_and_rejects_drift(self):
  with tempfile.TemporaryDirectory() as directory:
   binary=Path(directory)/"mcporter"; payload={"data":{"inline":{"items":[]}}}; binary.write_text(f"#!/bin/sh\nprintf '%s\\n' '{json.dumps({'content':[{'type':'text','text':json.dumps(payload)}]})}'\n"); binary.chmod(0o700)
   self.assertEqual([],run.Mcp(binary,"axon.axon",2,os.environ.copy()).call({"action":"collections","subaction":"list"})["items"])
   binary.write_text("#!/bin/sh\nprintf '[]\\n'\n")
   with self.assertRaisesRegex(run.RunnerError,"not an object"): run.Mcp(binary,"axon.axon",2,os.environ.copy()).call({"action":"capabilities"})
 def test_http_process_uses_exact_json_dto(self):
  seen=[]
  class Api(BaseHTTPRequestHandler):
   def log_message(self,*_): pass
   def do_POST(self):
    body=json.loads(self.rfile.read(int(self.headers.get("Content-Length","0")))); seen.append((self.path,body)); raw=json.dumps({"upload_id":"upl_1"}).encode(); self.send_response(200); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(raw))); self.end_headers(); self.wfile.write(raw)
  server=ThreadingHTTPServer(("127.0.0.1",0),Api); thread=threading.Thread(target=server.serve_forever); thread.start()
  try: self.assertEqual("upl_1",run.Http(f"http://127.0.0.1:{server.server_port}",None,2)("POST","/v1/uploads",{"filename":"x"})["upload_id"])
  finally: server.shutdown(); server.server_close(); thread.join()
  self.assertEqual([("/v1/uploads",{"filename":"x"})],seen)
 def test_handoff_requires_exact_command_and_complete_residual_accounting(self):
  with tempfile.TemporaryDirectory() as directory:
   directory=Path(directory); manifest=directory/"manifest.jsonl"; manifest.write_text("unused\n")
   report=directory/"report.json"; script=directory/"audit.py"
   script.write_text("#!/usr/bin/env python3\nimport json,sys\nfrom pathlib import Path\nPath(sys.argv[3]).write_text(json.dumps({'success':False,'created':[{'opaque_id':'x'}],'residual':[{'opaque_id':'x'}]}))\nraise SystemExit(2)\n"); script.chmod(0o700)
   old=run.ROOT
   try:
    run.ROOT=directory; expected=directory/"scripts/e2e/lib/residual-audit.py"; expected.parent.mkdir(parents=True); expected.write_text(script.read_text()); expected.chmod(0o700)
    handoff=directory/"handoff.json"; handoff.write_text(json.dumps({"command":[str(expected),str(manifest),"--report",str(report)],"report":str(report)}))
    self.assertFalse(run.execute_residual_handoff(handoff,manifest)["success"])
   finally: run.ROOT=old
if __name__=="__main__": unittest.main()
