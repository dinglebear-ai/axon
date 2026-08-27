import importlib.util, json, threading, unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SPEC=importlib.util.spec_from_file_location("mcp_auth",ROOT/"scripts/e2e/adapters/mcp_auth.py")
auth=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(auth)
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_POST(self):
        bearer=self.headers.get("authorization"); key=self.headers.get("x-api-key"); origin=self.headers.get("origin")
        status=200
        if bearer is None: status=401
        elif bearer not in {"Bearer good","Bearer read"}: status=401
        elif key: status=403
        elif origin == "https://denied.invalid": status=403
        length=int(self.headers.get("content-length","0")); payload=json.loads(self.rfile.read(length))
        if bearer == "Bearer read" and payload.get("method")=="tasks/cancel": status=403
        self.send_response(status); self.send_header("content-type","application/json"); self.end_headers()
        self.wfile.write(b'{"error":"unauthorized"}' if status in {401,403} else b'{"jsonrpc":"2.0","id":1,"result":{}}')
class AuthMatrixTests(unittest.TestCase):
    def test_matrix_executes_negative_scope_origin_and_conflict_cases(self):
        server=ThreadingHTTPServer(("127.0.0.1",0),Handler); thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
        try:
            value=auth.matrix(f"http://127.0.0.1:{server.server_port}/mcp","good","read","https://allowed.invalid")
            self.assertTrue(value["success"],value["failures"]); self.assertEqual(18,len(value["cases"]))
        finally: server.shutdown(); server.server_close(); thread.join()
if __name__=="__main__": unittest.main()
