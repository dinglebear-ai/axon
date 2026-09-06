from __future__ import annotations
import importlib.util,json,sys,tempfile,unittest
from pathlib import Path
from unittest import mock

ROOT=Path(__file__).resolve().parents[3]
SPEC=importlib.util.spec_from_file_location("restore_history_test",ROOT/"scripts/e2e/restore-reliability-history.py")
restore=importlib.util.module_from_spec(SPEC);sys.modules[SPEC.name]=restore;SPEC.loader.exec_module(restore)

class Result:
 def __init__(self,returncode=0,stdout="",stderr=""):self.returncode=returncode;self.stdout=stdout;self.stderr=stderr

class ReliabilityHistoryTests(unittest.TestCase):
 def test_failed_latest_artifact_is_skipped_and_previous_success_restores(self):
  runs=[{"databaseId":12,"attempt":1,"conclusion":"failure","headBranch":"main","headSha":"b"*40,"event":"push"},{"databaseId":11,"attempt":1,"conclusion":"success","headBranch":"main","headSha":"a"*40,"event":"push"}]
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);out=root/"out.json";att=root/"att.json"
   def command(argv,**_kwargs):
    if argv[:3]==["gh","run","list"]:return Result(stdout=json.dumps(runs))
    if argv[:3]==["gh","run","download"]:
     target=Path(argv[argv.index("--dir")+1]);(target/"history.json").write_text(json.dumps({"schema":1,"repository":"dinglebear-ai/axon","workflow":"e2e-hermetic.yml","trusted_ref":"refs/heads/main","reports":[]}));return Result()
    self.fail(f"unexpected command: {argv}")
   with mock.patch.object(restore.subprocess,"run",side_effect=command),mock.patch.object(sys,"argv",["restore","--repository","dinglebear-ai/axon","--workflow","e2e-hermetic.yml","--artifact","history","--out",str(out),"--attestations-out",str(att)]):self.assertEqual(0,restore.main())
   self.assertEqual([],json.loads(out.read_text())["reports"]);self.assertEqual([],json.loads(att.read_text())["runs"])

 def test_platform_workflow_supplies_platform_evidence_identity_template(self):
  text=(ROOT/".github/workflows/e2e-platform-smoke.yml").read_text()
  self.assertIn("--workflow e2e-platform-smoke.yml",text);self.assertIn("e2e-platform-smoke-${{ runner.os }}-{run_id}-{run_attempt}",text)

if __name__=="__main__":unittest.main()
