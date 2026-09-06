#!/usr/bin/env python3
"""Route one launcher allocation through the signed canonical teardown engine."""
from __future__ import annotations
import hashlib,importlib.util,json,socket,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def load_teardown():
 spec=importlib.util.spec_from_file_location("axon_e2e_descriptor_teardown",ROOT/"scripts/e2e/lib/teardown.py")
 if spec is None or spec.loader is None:raise RuntimeError("canonical teardown module unavailable")
 module=importlib.util.module_from_spec(spec);sys.modules[spec.name]=module;spec.loader.exec_module(module);return module
def validate_descriptor(descriptor_path,descriptor):
 if descriptor.get("schema")!=1 or descriptor.get("status") not in {"launching","running","verified"}:raise RuntimeError("invalid launcher descriptor")
 required=("run_id","run_root","ownership_manifest","cleanup_report","ports")
 if any(key not in descriptor for key in required):raise RuntimeError("launcher descriptor is incomplete")
 teardown=load_teardown();manifest=Path(descriptor["ownership_manifest"]).resolve();header,resources=teardown.manifest_api.load(manifest)
 if descriptor["run_id"]!=header.run_id:raise RuntimeError("descriptor run id disagrees with signed manifest")
 if Path(descriptor["run_root"]).resolve()!=header.data_dir.parent.resolve():raise RuntimeError("descriptor run root disagrees with signed manifest")
 if Path(descriptor["cleanup_report"]).resolve()!=manifest.parent/"cleanup-report.json":raise RuntimeError("descriptor cleanup report is not manifest-bound")
 signed_ports=sorted(int(item.identity) for item in resources if item.resource_type=="port")
 if sorted(map(int,descriptor["ports"]))!=signed_ports:raise RuntimeError("descriptor ports disagree with signed manifest")
 return manifest
def main():
 descriptor_path=Path(sys.argv[1]).resolve();descriptor=json.loads(descriptor_path.read_text());manifest=validate_descriptor(descriptor_path,descriptor);report_path=Path(descriptor["cleanup_report"])
 completed=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/lib/teardown.py"),str(manifest),"--report",str(report_path)],cwd=ROOT,capture_output=True,text=True,timeout=90,check=False)
 report=json.loads(report_path.read_text()) if report_path.is_file() else {"success":False,"fatal":"canonical teardown report absent"}
 for port in descriptor["ports"]:
  with socket.socket() as probe:
   if probe.connect_ex(("127.0.0.1",int(port)))==0:
    report.setdefault("residual",[]).append({"class":"port","opaque_id":hashlib.sha256(f"port\0{port}".encode()).hexdigest()[:20],"reason":"exact endpoint remains reachable"})
 report["success"]=completed.returncode==0 and not report.get("refused") and not report.get("residual")
 report_path.write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
 print(json.dumps(report,sort_keys=True));return 0 if report["success"] else 2
if __name__=="__main__":raise SystemExit(main())
