#!/usr/bin/env python3
"""Compatibility wrapper around the signed canonical teardown engine."""
import json,subprocess,sys
from pathlib import Path
def main():
 path=Path(sys.argv[1]).resolve();value=json.loads(path.read_text())
 if value.get("schema")!=1 or value.get("status") not in {"running","verified"}:raise RuntimeError("invalid live launcher descriptor")
 root=Path(__file__).resolve().parents[2]
 completed=subprocess.run([sys.executable,str(root/"scripts/e2e/teardown-hermetic-stack.py"),str(path)],cwd=root,check=False)
 return completed.returncode
if __name__=="__main__":raise SystemExit(main())
