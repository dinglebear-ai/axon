import hashlib, json, subprocess, sys, tempfile, unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
SCRIPT=ROOT/"scripts/e2e/assemble-qualification-input.py"
SHA="a"*40

class AssembleQualificationInputTests(unittest.TestCase):
 def test_assembly_copies_and_hashes_exact_sha_evidence(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary); evidence=root/"report.json"; evidence.write_text(json.dumps({"tested_sha":SHA,"safe":True}))
   producer={"repository":"dinglebear-ai/axon","workflow":"e2e-hermetic.yml","ref":"refs/heads/main","run_id":"12","run_attempt":1,"tested_sha":SHA,"completed_at":"2026-08-30T10:00:00Z"}
   assembly={"workflow":".github/workflows/e2e-qualification-assemble.yml","run_id":"99","run_attempt":1,"tested_sha":SHA}
   spec={"tested_sha":SHA,"profile":"release-candidate","policy_version":"1.0.0","as_of":"2026-08-30T11:00:00Z","product_version":"7.2.23","assembly":assembly,"artifacts":[{"id":"hermetic","family":"hermetic","source":str(evidence),"format":"canonical-report","producer":producer}]}
   spec_path=root/"spec.json";spec_path.write_text(json.dumps(spec));out=root/"bundle"
   subprocess.run([sys.executable,str(SCRIPT),"--spec",str(spec_path),"--out",str(out)],cwd=ROOT,check=True)
   index=json.loads((out/"qualification-index.json").read_text());item=index["artifacts"][0];copied=out/item["path"]
   self.assertEqual(hashlib.sha256(copied.read_bytes()).hexdigest(),item["sha256"]);self.assertEqual(SHA,index["tested_sha"])
   attestation=out/index["assembly_attestation"]["path"];self.assertEqual(hashlib.sha256(attestation.read_bytes()).hexdigest(),index["assembly_attestation"]["sha256"])
 def test_mismatched_artifact_sha_is_rejected(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);evidence=root/"report.json";evidence.write_text(json.dumps({"tested_sha":"b"*40}))
   producer={"tested_sha":SHA};assembly={"workflow":".github/workflows/e2e-qualification-assemble.yml","run_id":"99","tested_sha":SHA};spec={"tested_sha":SHA,"profile":"pr","policy_version":"1.0.0","as_of":"2026-08-30T11:00:00Z","product_version":"7","assembly":assembly,"artifacts":[{"id":"x","family":"hermetic","source":str(evidence),"format":"canonical-report","producer":producer}]}
   path=root/"spec.json";path.write_text(json.dumps(spec))
   result=subprocess.run([sys.executable,str(SCRIPT),"--spec",str(path),"--out",str(root/"out")],cwd=ROOT,capture_output=True,text=True)
   self.assertNotEqual(0,result.returncode);self.assertIn("artifact tested SHA mismatch",result.stderr)

if __name__=="__main__":unittest.main()
