#!/usr/bin/env python3
"""Trust-gated workflow entry for the real two-allocation stateful pack."""
import os,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
if os.environ.get("AXON_E2E_TRUSTED_LIVE") != "1":
 raise SystemExit("stateful live E2E requires AXON_E2E_TRUSTED_LIVE=1")
required=("AXON_E2E_REAL_AXON_BIN","AXON_E2E_MCPORTER","AXON_E2E_FIXTURE_SOURCE","AXON_E2E_FIXTURE_SOURCE_ID")
missing=[name for name in required if not os.environ.get(name)]
if missing: raise SystemExit("missing trusted live inputs: "+",".join(missing))
argv=[sys.executable,str(Path(__file__).with_name("run.py")),"--axon-bin",os.environ[required[0]],"--mcporter",os.environ[required[1]],
 "--launcher",str(ROOT/"scripts/e2e/launch-hermetic-stack.py"),"--fixture-source",os.environ[required[2]],"--fixture-source-id",os.environ[required[3]],
 "--work-root",os.environ.get("AXON_E2E_STATEFUL_ROOT",str(ROOT/"target/e2e/stateful-live"))]
raise SystemExit(subprocess.run(argv,cwd=ROOT,check=False).returncode)
