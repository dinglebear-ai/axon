import re, subprocess, sys, unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3];DOC=ROOT/"docs/guides/e2e-operations.md"

class E2EOperationsDocTests(unittest.TestCase):
 def test_required_topics_and_no_secret_examples(self):
  text=DOC.read_text()
  headings=("Trust boundaries and execution lanes","Local setup and ordinary operation","Required hermetic check","Live WIF, grants, and provider gateways","Failure taxonomy and triage","Evidence, redaction, and qualification","Teardown, cancellation, and stale recovery","Troubleshooting","Outage bypass, rollback, quarantine, and release")
  for heading in headings:self.assertIn(f"## {heading}",text)
  self.assertNotRegex(text,re.compile(r"(?i)(?:token|password|secret)\s*[=:]\s*[A-Za-z0-9_-]{8,}"))
  self.assertIn("780049a30b6ff5c378a9e7b389d15ece7a204888",text);self.assertIn("1.94.0",text)
 def test_referenced_paths_exist(self):
  paths=set(re.findall(r"`((?:scripts|config|tests|docs|\.github)/[^` ]+)`",DOC.read_text()))
  self.assertEqual([],sorted(path for path in paths if not (ROOT/path.rstrip(".,;:")).exists()))
 def test_documented_just_recipes_exist(self):
  documented=set(re.findall(r"just (e2e-[a-z-]+)",DOC.read_text()))
  result=subprocess.run(["just","--summary"],cwd=ROOT,capture_output=True,text=True,check=True)
  self.assertEqual(set(),documented-set(result.stdout.split()))
 def test_documented_script_entrypoints_parse_help(self):
  scripts=("scripts/e2e/run-live.py","scripts/e2e/run-mutations.py","scripts/e2e/run-upgrade.py","scripts/e2e/build-qualification-manifest.py","scripts/e2e/lib/teardown.py","scripts/e2e/lib/stale-janitor.py")
  for script in scripts:
   result=subprocess.run([sys.executable,str(ROOT/script),"--help"],cwd=ROOT,capture_output=True,text=True,timeout=10)
   self.assertEqual(0,result.returncode,f"{script}: {result.stderr}")

if __name__=="__main__":unittest.main()
