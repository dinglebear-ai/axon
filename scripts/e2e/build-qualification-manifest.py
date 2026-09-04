#!/usr/bin/env python3
from __future__ import annotations
import argparse, hashlib, importlib.util, json, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("axon_qualification", ROOT / "scripts/e2e/lib/qualification.py")
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)

def main() -> int:
    parser=argparse.ArgumentParser(); parser.add_argument("--index",type=Path,required=True); parser.add_argument("--policy",type=Path,default=ROOT/"config/e2e/qualification-policy.json"); parser.add_argument("--evidence-root",type=Path,required=True); parser.add_argument("--out",type=Path,required=True); parser.add_argument("--summary",type=Path,required=True); parser.add_argument("--checksums",type=Path,required=True); args=parser.parse_args()
    try: manifest, checksum=module.build(json.loads(args.index.read_text()),json.loads(args.policy.read_text()),args.evidence_root)
    except (OSError,json.JSONDecodeError,module.QualificationError) as error: print(f"qualification failed: {error}",file=sys.stderr); return 2
    args.out.parent.mkdir(parents=True,exist_ok=True)
    manifest_bytes=(json.dumps(manifest,indent=2,sort_keys=True)+"\n").encode(); manifest_file_digest=hashlib.sha256(manifest_bytes).hexdigest(); summary_bytes=module.summary(manifest,manifest_file_digest).encode()
    args.out.write_bytes(manifest_bytes); args.summary.write_bytes(summary_bytes)
    args.checksums.write_text(f"{manifest_file_digest}  {args.out.name}\n{hashlib.sha256(summary_bytes).hexdigest()}  {args.summary.name}\n")
    return 0 if manifest["qualification"]["outcome"] == "passed" else 2
if __name__ == "__main__": raise SystemExit(main())
