#!/usr/bin/env python3
"""Stateful Qdrant/Chrome boundary double whose denials are provable no-ops."""
import argparse,hashlib,json
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*_):pass
 def reply(self,status,value):
  body=json.dumps(value).encode();self.send_response(status);self.send_header("content-type","application/json");self.send_header("content-length",str(len(body)));self.end_headers();self.wfile.write(body)
 def handle_request(self):
  path=self.path.split("?",1)[0]
  if path=="/stats":return self.reply(200,{"calls":self.server.calls,"sinks":self.server.sinks})
  if path=="/state":
   encoded=json.dumps(self.server.resources,sort_keys=True,separators=(",",":")).encode()
   return self.reply(200,{"resources":self.server.resources,"sha256":hashlib.sha256(encoded).hexdigest()})
  self.server.calls.append({"method":self.command,"path":path})
  if path=="/redirect":
   self.server.sinks["redirect"]+=1;self.send_response(302);self.send_header("location","http://169.254.169.254/latest/meta-data/");self.end_headers();return
  if path=="/rebind":
   self.server.sinks["rebind"]+=1;return self.reply(200,{"error":{"code":"ssrf.rebind_sink_reached"}})
  if path.startswith("/active/"):
   return self.reply(409,{"error":{"code":"provider.resource_active","message":"active owned resources cannot be deleted"}})
  qdrant=path.startswith(("/collections/","/aliases","/snapshots","/cluster"))
  chrome=path.startswith(("/profiles/","/sessions/","/json","/admin"))
  if qdrant or chrome:return self.reply(403,{"error":{"code":"provider.not_owned","message":"owned E2E marker required"}})
  return self.reply(404,{"error":{"code":"route.not_found"}})
 do_GET=do_POST=do_PUT=do_DELETE=handle_request
def main():
 p=argparse.ArgumentParser();p.add_argument("--port",type=int,required=True);a=p.parse_args()
 s=ThreadingHTTPServer(("127.0.0.1",a.port),Handler);s.calls=[];s.sinks={"redirect":0,"rebind":0}
 s.resources={"qdrant":{"collections":["production"],"aliases":["live"],"snapshots":["snap"]},
              "chrome":{"profiles":["operator-profile"],"sessions":["operator-session"]}}
 s.serve_forever()
if __name__=="__main__":main()
