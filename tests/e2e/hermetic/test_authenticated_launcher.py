from __future__ import annotations
import hashlib,json,os,secrets,stat,subprocess,sys,tempfile,unittest,urllib.error,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]

class AuthenticatedLauncherTests(unittest.TestCase):
 def test_partial_launcher_setup_runs_canonical_teardown(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);run_id=f"axon_e2e_{secrets.token_hex(12)}";run_root=root/run_id
   allocation={"run_id":run_id,"collection":run_id,"run_root":str(run_root),"ownership_generation":secrets.token_hex(32)}
   env={**os.environ,"AXON_E2E_REAL_AXON_BIN":str(root/"missing-axon")}
   launched=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/launch-hermetic-stack.py")],input=json.dumps(allocation),
                            cwd=ROOT,env=env,text=True,capture_output=True,timeout=30)
   self.assertNotEqual(0,launched.returncode);self.assertFalse(run_root.exists())
   cleanup=root/"ownership-manifests"/run_id/"cleanup-report.json"
   self.assertTrue(cleanup.is_file());self.assertTrue(json.loads(cleanup.read_text())["success"])

 @unittest.skipUnless((ROOT/"target/debug/axon").is_file(),"built Axon required")
 def test_token_is_per_allocation_private_and_authenticates_http_and_mcp(self):
  descriptors=[]
  with tempfile.TemporaryDirectory() as directory:
   try:
    for index in range(2):
     run_id=f"axon_e2e_{secrets.token_hex(12)}";allocation={"run_id":run_id,"collection":run_id,
      "run_root":str(Path(directory)/run_id),"ownership_generation":secrets.token_hex(32)}
     launched=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/launch-hermetic-stack.py")],input=json.dumps(allocation),
                              cwd=ROOT,text=True,capture_output=True,timeout=30,check=True)
     descriptor=json.loads(launched.stdout);descriptors.append(descriptor);path=Path(descriptor["descriptor_path"])
     self.assertEqual(stat.S_IMODE(path.stat().st_mode),0o600)
     token=descriptor["environment"]["AXON_HTTP_TOKEN"]
     self.assertGreaterEqual(len(token),48);self.assertEqual(hashlib.sha256(token.encode()).hexdigest(),descriptor["http_token_sha256"])
     with self.assertRaises(urllib.error.HTTPError) as rejected:urllib.request.urlopen(descriptor["http_base_url"]+"/v1/status",timeout=2)
     try:self.assertEqual(rejected.exception.code,401)
     finally:rejected.exception.close()
     request=urllib.request.Request(descriptor["http_base_url"]+"/v1/status",headers={"Authorization":f"Bearer {token}"})
     with urllib.request.urlopen(request,timeout=2) as response:self.assertEqual(response.status,200)
     env={**os.environ,**descriptor["environment"]}
     mcp=subprocess.run(["mcporter","call",descriptor["mcp_selector"],"--args",json.dumps({"action":"capabilities"}),"--output","json"],
                        cwd=ROOT,env=env,text=True,capture_output=True,timeout=15)
     self.assertEqual(mcp.returncode,0,mcp.stderr)
    self.assertNotEqual(descriptors[0]["environment"]["AXON_HTTP_TOKEN"],descriptors[1]["environment"]["AXON_HTTP_TOKEN"])
   finally:
    for descriptor in descriptors:
     subprocess.run(descriptor["teardown_handle"]["command"],cwd=ROOT,capture_output=True,text=True,timeout=15)

 def test_retained_descriptor_contains_only_redaction_and_digest(self):
  retained=ROOT/"target/e2e/launcher-descriptor.json"
  if not retained.is_file():self.skipTest("real composed evidence not generated")
  value=json.loads(retained.read_text());self.assertEqual(value["environment"]["AXON_HTTP_TOKEN"],"[REDACTED]")
  self.assertEqual(value["bindings"]["AXON_HTTP_TOKEN"],"[REDACTED]")
  self.assertRegex(value["http_token_sha256"],r"^[0-9a-f]{64}$")

if __name__=="__main__":unittest.main()
