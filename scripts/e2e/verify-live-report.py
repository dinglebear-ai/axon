#!/usr/bin/env python3
import importlib.util,json,re,sys
from pathlib import Path
path=Path(sys.argv[1])
if not path.is_file():raise SystemExit("live evidence absent; cleanup is unproven")
report=json.loads(path.read_text())
encoded=json.dumps(report,sort_keys=True).encode()
spec=importlib.util.spec_from_file_location("live_report_redaction",Path(__file__).with_name("lib")/"redaction.py");redaction=importlib.util.module_from_spec(spec);spec.loader.exec_module(redaction)
redaction.scan_bytes(encoded,())
if re.search(rb"(?i)(?:[a-z0-9-]+\.)+ts\.net\.?",encoded):raise SystemExit("private tailnet hostname found in live evidence")
if report.get("sanitized") is not True or report.get("success") is not True:raise SystemExit("live result or sanitization failed")
if report.get("cleanup")!=[{"provider":"canonical-teardown","passed":True}]:raise SystemExit("live residual audit failed")
teardown=report.get("teardown",{})
if teardown.get("success") is not True or teardown.get("residual") or teardown.get("refused") or not teardown.get("phases"):raise SystemExit("canonical teardown receipt failed")
if report.get("classification") is not None:raise SystemExit("live failure classification present")
