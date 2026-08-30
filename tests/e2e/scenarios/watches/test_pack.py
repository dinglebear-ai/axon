import importlib.util, threading, unittest
from pathlib import Path
S=importlib.util.spec_from_file_location("watch_pack",Path(__file__).with_name("pack.py")); p=importlib.util.module_from_spec(S); S.loader.exec_module(p)
class Tests(unittest.TestCase):
 def test_duplicate_tick_linkage(self):
  lock=threading.Lock()
  def cli(*a,**k):
   with lock:
    if a[0]=="jobs": return {"summary":{"id":"j1","kind":"source"},"stages":[],"artifacts":[]}
    op=a[1]
    if op=="create": return {"watch_id":"w1"}
    if op=="exec": return {"watch_id":"w1","job_id":"j1"}
    if op=="get" and k.get("ok") is not False: return {"watch_id":"w1","request":{"watch_id":"w1","schedule":{"every_seconds":120}}}
    if op=="status": return {"watch":{"enabled":hasattr(cli,"resumed")},"latest_job_summary":None}
    if op=="resume": cli.resumed=True; return {"status":"active"}
    if op=="history": return {"watch_id":"w1","jobs":[{"id":"j1","kind":"source"}],"next_cursor":None}
    if op=="get" and k.get("ok") is False: return {"code":"watch.not_found"}
    return {"ok":True}
  registered=[]; result=p.run(cli,"fixture","run",lambda k,i:registered.append((k,i))); self.assertEqual("j1",result["job_id"]); self.assertEqual([("watch","w1"),("job","j1")],registered)
if __name__=="__main__": unittest.main()
