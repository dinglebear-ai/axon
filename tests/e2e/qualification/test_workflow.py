import re,unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
WORKFLOW=ROOT/".github/workflows/e2e-qualification.yml"

class QualificationWorkflowTests(unittest.TestCase):
 def test_reusable_and_dispatch_automation_is_least_privilege_and_sha_bound(self):
  text=WORKFLOW.read_text();self.assertIn("workflow_call:",text);self.assertIn("workflow_dispatch:",text)
  self.assertIn("contents: read\n  actions: read",text);self.assertNotIn("contents: write",text);self.assertNotIn("id-token: write",text)
  self.assertIn("ref: ${{ inputs.tested_sha }}",text);self.assertIn('index.get("tested_sha") != expected_sha',text)
  self.assertIn('index.get("profile") != os.environ["EXPECTED_PROFILE"]',text)
  self.assertIn("name: e2e-qualification-input",text);self.assertIn("run-id: ${{ inputs.evidence_run_id }}",text)
  self.assertIn("actions/runs/${EVIDENCE_RUN_ID}",text);self.assertIn("test \"$actual_sha\" = \"$EXPECTED_SHA\"",text);self.assertIn("test \"$conclusion\" = success",text)
  self.assertIn('test "$EVIDENCE_RUN_ID" = "$GITHUB_RUN_ID"',text);self.assertIn('test "$EXPECTED_SHA" = "$GITHUB_SHA"',text)
  for action in re.findall(r"uses:\s*([^\s]+)",text):self.assertRegex(action,r"^[^@]+@[0-9a-f]{40}$")
 def test_output_contract_contains_all_three_unsigned_artifacts(self):
  text=WORKFLOW.read_text()
  for name in ("qualification.json","qualification.md","SHA256SUMS"):self.assertIn(name,text)
  self.assertIn("default: release-candidate",text);self.assertIn("retention-days: 30",text)

if __name__=="__main__":unittest.main()
