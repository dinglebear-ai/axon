#!/usr/bin/env python3
"""Owned OAuth issuer exercising redirect/state/PKCE/scopes/claims."""
import argparse,base64,hashlib,hmac,json,secrets,urllib.parse
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
def b64(value):return base64.urlsafe_b64encode(value).decode().rstrip("=")
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*_):pass
 def body(self,status,value,headers=()):
  data=json.dumps(value).encode();self.send_response(status)
  for key,item in headers:self.send_header(key,item)
  self.send_header("content-type","application/json");self.send_header("content-length",str(len(data)));self.end_headers();self.wfile.write(data)
 def do_GET(self):
  parsed=urllib.parse.urlsplit(self.path);query=urllib.parse.parse_qs(parsed.query)
  if parsed.path=="/.well-known/oauth-authorization-server":return self.body(200,{"issuer":self.server.issuer,"authorization_endpoint":self.server.issuer+"/authorize","token_endpoint":self.server.issuer+"/token","jwks_uri":self.server.issuer+"/jwks","scopes_supported":["axon:read","axon:write"],"code_challenge_methods_supported":["S256"]})
  if parsed.path=="/jwks":return self.body(200,{"keys":[{"kty":"oct","kid":"e2e","alg":"HS256","k":b64(self.server.secret)}]})
  if parsed.path!="/authorize":return self.body(404,{"error":"not_found"})
  required=("client_id","redirect_uri","state","code_challenge","code_challenge_method","scope")
  if any(not query.get(key,[""])[0] for key in required):return self.body(400,{"error":"invalid_request"})
  redirect=query["redirect_uri"][0];scope=query["scope"][0];state=query["state"][0]
  if any(character in redirect or character in state for character in ("\r","\n")):return self.body(400,{"error":"invalid_request"})
  if redirect not in self.server.redirects:return self.body(400,{"error":"invalid_redirect_uri"})
  if query["code_challenge_method"][0]!="S256" or scope not in {"axon:read","axon:write"}:return self.body(400,{"error":"invalid_request"})
  code=secrets.token_urlsafe(20);self.server.codes[code]={"challenge":query["code_challenge"][0],"scope":scope,"client_id":query["client_id"][0],"redirect":redirect}
  location=redirect+"?"+urllib.parse.urlencode({"code":code,"state":state})
  self.send_response(302);self.send_header("location",location);self.end_headers()
 def do_POST(self):
  if self.path!="/token":return self.body(404,{"error":"not_found"})
  length=int(self.headers.get("content-length","0"));form=urllib.parse.parse_qs(self.rfile.read(length).decode());code=form.get("code",[""])[0]
  row=self.server.codes.pop(code,None);verifier=form.get("code_verifier",[""])[0]
  challenge=b64(hashlib.sha256(verifier.encode()).digest())
  if not row or challenge!=row["challenge"] or form.get("redirect_uri",[""])[0]!=row["redirect"]:return self.body(400,{"error":"invalid_grant"})
  claims={"iss":self.server.issuer,"sub":"e2e-user","email":"e2e@example.invalid","email_verified":True,"scope":row["scope"]}
  head=b64(b'{"alg":"HS256","kid":"e2e","typ":"JWT"}');payload=b64(json.dumps(claims,separators=(",",":")).encode());sig=b64(hmac.new(self.server.secret,f"{head}.{payload}".encode(),hashlib.sha256).digest())
  self.body(200,{"access_token":f"{head}.{payload}.{sig}","token_type":"Bearer","scope":row["scope"]})
def main():
 p=argparse.ArgumentParser();p.add_argument("--port",type=int,required=True);a=p.parse_args();s=ThreadingHTTPServer(("127.0.0.1",a.port),Handler)
 s.issuer=f"http://127.0.0.1:{a.port}";s.redirects={"http://127.0.0.1/callback"};s.secret=secrets.token_bytes(32);s.codes={};s.serve_forever()
if __name__=="__main__":main()
