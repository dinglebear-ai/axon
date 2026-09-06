#!/usr/bin/env python3
"""File-backed faithful CLI contract for Docker, Compose, Tailscale, and Axon."""
from __future__ import annotations
import json, os, sys
from pathlib import Path
import signal, time

path = Path(os.environ["AXON_E2E_PROVIDER_STATE"])
state = json.loads(path.read_text())
binary = Path(sys.argv[0]).name; args = sys.argv[1:]
failure_token = binary + ":" + ":".join(args[:2])
if state.get("fail_next") in {binary, failure_token}:
    state["fail_next"] = None; path.write_text(json.dumps(state)); raise SystemExit(73)

def save(): path.write_text(json.dumps(state, sort_keys=True))
def docker():
    if args[:1] == ["compose"]:
        if args[1:3] == ["ls", "--all"]: print(json.dumps(list(state["compose"].values()))); return
        project = args[args.index("-p") + 1]
        if "ps" in args: print(json.dumps(state["compose"].get(project, []))); return
        if "down" in args: state["compose"].pop(project, None); save(); return
    kind, operation = args[0], args[1]; store = state["docker"][kind]
    if operation == "create":
        label = args[args.index("--label") + 1].split("=", 1)[1]
        identity = args[args.index("--name") + 1] if kind == "container" else args[-1]
        store[identity] = ({"Id": identity, "Config": {"Labels": {"axon.e2e.ownership": label}}}
                           if kind == "container" else {"Id": identity, "Name": identity, "Labels": {"axon.e2e.ownership": label}})
        save(); print(identity); return
    if operation == "inspect":
        identity=args[-1]
        if identity not in store: raise SystemExit(1)
        print(json.dumps([store[identity]])); return
    if operation in {"rm"}:
        identity=args[-1]
        if identity not in store: raise SystemExit(1)
        del store[identity];save();return
    if operation == "ls":
        for identity in store: print(identity)
def tailscale():
    if len(args) >= 4 and args[:1] == ["--socket"] and args[2:] == ["status", "--json"]:
        print(json.dumps(state["tailscale"])); return
    if args[-1:] == ["logout"]:
        if args[:1] != ["--socket"]: raise SystemExit(64)
        state["tailscale"]["BackendState"] = "Stopped"; save(); return
def tailscaled():
    state_file=Path(args[args.index("--state")+1]);socket_file=Path(args[args.index("--socket")+1])
    state_file.write_text("faithful-isolated-state");socket_file.touch()
    stopping=False
    def stop(_sig,_frame):
        nonlocal stopping;stopping=True
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    while not stopping: time.sleep(.05)
    socket_file.unlink(missing_ok=True)
def axon():
    values=[value for value in args if value != "--json"]; family,operation=values[:2]; store=state[family]
    if operation == "list": print(json.dumps({"items":list(store.values())}));return
    identity=values[2]
    if operation == "get":
        if identity not in store: raise SystemExit(1)
        print(json.dumps(store[identity]));return
    if (family,operation) in {("watch","delete"),("uploads","abort")}:
        if identity not in store: raise SystemExit(1)
        del store[identity];save();print(json.dumps({"deleted":identity}));return
if binary == "docker": docker()
elif binary == "tailscale": tailscale()
elif binary == "tailscaled": tailscaled()
elif binary == "axon": axon()
else: raise SystemExit(64)
