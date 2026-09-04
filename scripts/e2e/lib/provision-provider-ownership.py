#!/usr/bin/env python3
"""Create/bind exact provider ownership before a resource is used."""
from __future__ import annotations
import argparse, importlib.util, json, sys
from pathlib import Path
def load(name, filename):
    path=Path(__file__).with_name(filename); spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);sys.modules[name]=module;spec.loader.exec_module(module);return module
manifest_api=load("axon_e2e_bind_manifest","resource-manifest.py")
providers=load("axon_e2e_bind_providers","provider-adapters.py")
def main():
    parser=argparse.ArgumentParser();parser.add_argument("manifest",type=Path);parser.add_argument("resource_type");parser.add_argument("identity")
    parser.add_argument("--provider-config",type=Path,required=True);parser.add_argument("--report",type=Path,required=True);args=parser.parse_args()
    try:
        header,resources=manifest_api.load(args.manifest);matches=[r for r in resources if (r.resource_type,r.identity)==(args.resource_type,args.identity)]
        if len(matches)!=1: raise RuntimeError("resource identity is missing or ambiguous")
        adapters=providers.build(args.provider_config,header,manifest_api);adapter=adapters.get(args.resource_type)
        if adapter is None or not hasattr(adapter,"provision_ownership"): raise RuntimeError("resource adapter has no ownership provisioning contract")
        result=adapter.provision_ownership(matches[0]);report={"success":True,"result":result}
    except Exception as error:report={"success":False,"fatal":str(error)}
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
    return 0 if report.get("success") else 2
if __name__=="__main__":raise SystemExit(main())
