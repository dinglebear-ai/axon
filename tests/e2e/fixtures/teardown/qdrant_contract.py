"""File-backed loopback Qdrant REST contract used by teardown integration tests."""
from __future__ import annotations
import json
import argparse
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

class Handler(BaseHTTPRequestHandler):
    state_path = None
    state_lock = threading.RLock()
    def handle_one_request(self):
        with self.state_lock: super().handle_one_request()
    def log_message(self,*_args): pass
    def state(self): return json.loads(self.state_path.read_text())
    def save(self,state): self.state_path.write_text(json.dumps(state,sort_keys=True))
    def body(self):
        length=int(self.headers.get("content-length","0"));return json.loads(self.rfile.read(length) or b"{}")
    def fail(self):
        state=self.state(); token=f"{self.command} {urlparse(self.path).path}"
        if state.get("fail_next") == token:
            state["fail_next"]=None;self.save(state);self.reply({"error":"injected provider outage"},503);return True
        return False
    def reply(self,result,code=200):
        raw=json.dumps({"result":result}).encode();self.send_response(code);self.send_header("content-length",str(len(raw)));self.end_headers();self.wfile.write(raw)
    def do_GET(self):
        if self.fail(): return
        state=self.state(); parts=urlparse(self.path).path.strip("/").split("/")
        if parts == ["collections"]: return self.reply({"collections":[{"name":name} for name in state["collections"]]})
        if parts == ["aliases"]: return self.reply({"aliases":[{"alias_name":name,"collection_name":collection} for name,collection in state["aliases"].items()]})
        if len(parts)>=2 and parts[0]=="collections":
            collection=parts[1]
            if collection not in state["collections"]: return self.reply({},404)
            if len(parts)==2:
                item=state["collections"][collection]
                vectors={"dense":{"size":item["size"],"distance":"Cosine"}} if item.get("named") else {"size":item["size"],"distance":"Cosine"}
                params={"vectors":vectors}
                if item.get("sparse"): params["sparse_vectors"]={"bm42":{"modifier":"idf"}}
                return self.reply({"config":{"params":params},
                                   "payload_schema":item["indexes"],"optimizer_status":"ok"})
            if parts[2:]==["snapshots"]: return self.reply(list(state["collections"][collection]["snapshots"].values()))
        if parts in (["readyz"], ["healthz"]): return self.reply({"status":"ok"})
        self.reply({},404)
    def do_POST(self):
        if self.fail(): return
        state=self.state(); parts=urlparse(self.path).path.strip("/").split("/"); payload=self.body()
        if parts == ["collections","aliases"]:
            for action in payload["actions"]:
                if "delete_alias" in action: state["aliases"].pop(action["delete_alias"]["alias_name"],None)
                else:
                    item=action["create_alias"];state["aliases"][item["alias_name"]]=item["collection_name"]
            self.save(state);return self.reply(True)
        collection=parts[1]; item=state["collections"][collection]
        if parts[2:]==["snapshots"]:
            name=state.get("next_snapshot_name")
            if not name: return self.reply({"error":"fixture requires deterministic snapshot name"},400)
            item["snapshots"][name]={"name":name,"checksum":"created"};self.save(state);return self.reply({"name":name})
        if parts[2:]==["points","scroll"]:
            points=list(item["points"].values());offset=payload.get("offset");start=0
            if offset is not None:
                start=next((index+1 for index,value in enumerate(points) if str(value.get("id"))==str(offset)),len(points))
            selected=points[start:start+int(payload.get("limit",256))];next_offset=None
            if start+len(selected)<len(points):next_offset=selected[-1]["id"]
            return self.reply({"points":selected,"next_page_offset":next_offset})
        if parts[2:]==["points","query"]:
            limit=max(1,int(payload.get("limit",10)))
            points=[point for point in item["points"].values()
                    if point.get("payload",{}).get("resource_type") != "collection"][:limit]
            return self.reply({"points":[{"id":point["id"],"score":0.99,
                "payload":point.get("payload",{})} for point in points]})
        if parts[2:]==["points","count"]:
            conditions=payload.get("filter",{}).get("must",[])
            def matches(point):
                values=point.get("payload",{})
                return all(("value" not in condition.get("match",{}) or values.get(condition.get("key"))==condition["match"]["value"]) and ("any" not in condition.get("match",{}) or values.get(condition.get("key")) in condition["match"]["any"]) for condition in conditions)
            return self.reply({"count":sum(1 for point in item["points"].values() if matches(point))})
        if parts[2:]==["points"]:
            return self.reply([item["points"][str(identity)] for identity in payload["ids"] if str(identity) in item["points"]])
        if parts[2:]==["points","payload"]:
            identities=payload.get("points")
            if not isinstance(identities,list) and isinstance(payload.get("filter"),dict):
                conditions=payload["filter"].get("must",[])
                def matches(point):
                    values=point.get("payload",{})
                    return all(("value" not in condition.get("match",{}) or values.get(condition.get("key"))==condition["match"]["value"]) and ("any" not in condition.get("match",{}) or values.get(condition.get("key")) in condition["match"]["any"]) for condition in conditions)
                identities=[key for key,point in item["points"].items() if matches(point)]
            for identity in identities or []:
                if str(identity) in item["points"]: item["points"][str(identity)].setdefault("payload",{}).update(payload["payload"])
        elif parts[2:]==["points","delete"]:
            identities=payload.get("points")
            if isinstance(identities,list):
                for identity in identities: item["points"].pop(str(identity),None)
            elif isinstance(payload.get("filter"),dict):
                conditions=payload["filter"].get("must",[])
                def matches(point):
                    values=point.get("payload",{})
                    for condition in conditions:
                        key=condition.get("key"); match=condition.get("match",{})
                        if "value" in match and values.get(key)!=match["value"]: return False
                        if "any" in match and values.get(key) not in match["any"]: return False
                    return True
                for identity in [key for key,point in item["points"].items() if matches(point)]: item["points"].pop(identity,None)
        self.save(state);return self.reply({"status":"completed"})
    def do_PUT(self):
        if self.fail(): return
        state=self.state();parts=urlparse(self.path).path.strip("/").split("/");collection=parts[1];payload=self.body()
        if len(parts)==2:
            vectors=payload["vectors"]
            size=(vectors.get("dense",{}).get("size") if isinstance(vectors,dict) and "dense" in vectors
                  else vectors.get("size") if isinstance(vectors,dict) else None)
            state["collections"][collection]={"size":size,"named":isinstance(vectors,dict) and "dense" in vectors,
                "sparse":bool(payload.get("sparse_vectors")),"indexes":{},"snapshots":{},"points":{}}
            self.save(state);return self.reply(True)
        if parts[2]=="index":
            field_name=parts[3] if len(parts)>3 else payload.get("field_name")
            if not isinstance(field_name,str): return self.reply({"error":"payload index field_name required"},400)
            state["collections"][collection]["indexes"][field_name]=payload
            self.save(state);return self.reply({"status":"completed"})
        for point in payload["points"]: state["collections"][collection]["points"][str(point["id"])]=point
        self.save(state);self.reply({"status":"completed"})
    def do_DELETE(self):
        if self.fail(): return
        state=self.state();parts=urlparse(self.path).path.strip("/").split("/");collection=parts[1]
        if len(parts)==2: state["collections"].pop(collection,None)
        elif parts[2]=="snapshots": state["collections"][collection]["snapshots"].pop(parts[3],None)
        elif parts[2]=="index": state["collections"][collection]["indexes"].pop(parts[3],None)
        self.save(state);self.reply(True)

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--port",type=int,required=True)
    parser.add_argument("--state",type=__import__("pathlib").Path,required=True);args=parser.parse_args()
    Handler.state_path=args.state
    ThreadingHTTPServer(("127.0.0.1",args.port),Handler).serve_forever()

if __name__ == "__main__": main()
