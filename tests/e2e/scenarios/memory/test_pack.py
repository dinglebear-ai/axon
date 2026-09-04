import importlib.util,json,tempfile,unittest
from pathlib import Path
S=importlib.util.spec_from_file_location("memory_pack",Path(__file__).with_name("pack.py")); p=importlib.util.module_from_spec(S); S.loader.exec_module(p)

def item(i,body,status="active",access=0): return {"id":i,"memory_type":"fact","status":status,"body":body,"confidence":.9,"access_count":access}
class Fake:
 def __init__(self): self.r={}; self.n=0; self.imported=False
 def __call__(self,*a,**kw):
  assert a[0]=="memory" and a[-1]=="--json",a; op=a[1]
  if op=="remember":
   self.n+=1; i=f"mem_{self.n}"; self.r[i]=item(i,a[2]); return dict(self.r[i],graph_node_id=f"node_{self.n}",vector_point_ids=[f"point_{self.n}"])
  if op=="show": return dict(self.r[a[2]]) if a[2] in self.r else {"code":"memory.not_found","error":"not found"}
  if op=="link": return {"source_id":a[2],"target_id":a[3],"edge_type":"relates_to"}
  if op=="supersede": self.r[a[3]]["status"]="superseded"; return {"source_id":a[2],"target_id":a[3],"edge_type":"supersedes"}
  if op=="reinforce":
   assert a[3:6]==("--amount","0.25","--reason"); self.r[a[2]]["access_count"]+=1; return dict(self.r[a[2]])
  if op=="pin": return dict(self.r[a[2]])
  if op=="contradict": self.r[a[2]]["status"]="contradicted"; return {"source_id":a[2],"target_id":a[3],"edge_type":"contradicts"}
  if op=="archive": self.r[a[2]]["status"]="archived"; return dict(self.r[a[2]])
  if op=="export":
   values=[{"memory_id":v["id"],"memory_type":"fact","status":v["status"],"body":v["body"],"confidence":.9,"salience":.5,"scope":{"kind":"global","value":""},"history":[],"visibility":"private","embedding_refs":[f"point_export_{v['id']}"]} for v in self.r.values()]
   Path(a[3]).write_text(json.dumps(values)); return {"count":len(values)}
  if op=="import":
   record=json.loads(Path(a[2]).read_text())[0]
   if self.imported:return {"created":0,"updated":0,"skipped":1,"dry_run":False,"created_ids":[]}
   self.imported=True; self.r[record["memory_id"]]=item(record["memory_id"],record["body"]); return {"created":1,"updated":0,"skipped":0,"dry_run":False,"created_ids":[record["memory_id"]]}
  if op=="search": return [dict(v) for v in self.r.values() if v["status"]=="active" and a[2] in v["body"]]
  if op=="review": return {"memories":[dict(v) for v in self.r.values()],"warnings":[]}
  if op=="compact":
   assert a[-2]=="--archive-sources"
   for i in a[2:4]: self.r[i]["status"]="archived"
   self.r["mem_compact"]=item("mem_compact",self.r[a[2]]["body"]+self.r[a[3]]["body"]); return dict(self.r["mem_compact"])
  if op=="forget": self.r[a[2]].update(status="forgotten",body=""); return dict(self.r[a[2]])
  raise AssertionError(a)
class Tests(unittest.TestCase):
 def test_flow_and_recursive_registration(self):
  f=Fake(); registered=[]
  with tempfile.TemporaryDirectory() as d:r=p.run(f,"run_x",lambda k,i:registered.append((k,i)),d)
  self.assertEqual(["mem_1","mem_2","mem_3","mem_4"],r["ids"]); self.assertEqual("run_x_imported",r["imported_id"])
  for pair in [("graph_node","node_1"),("point","point_1"),("point","point_export_mem_1"),("memory_record","run_x_imported"),("memory_record","mem_compact")]:self.assertIn(pair,registered)
 def test_foreign_search_rejected(self):
  f=Fake()
  def contaminated(*a,**kw):
   value=f(*a,**kw)
   if a[1]=="search":value.append(item("foreign","run_x foreign"))
   return value
  with tempfile.TemporaryDirectory() as d:
   with self.assertRaisesRegex(p.MemoryContractError,"identity isolation"):p.run(contaminated,"run_x",lambda *_:None,d)
 def test_created_ids_are_mandatory(self):
  f=Fake()
  def broken(*a,**kw):
   value=f(*a,**kw)
   if a[1]=="import" and value.get("created")==1:value["created_ids"]=[]
   return value
  with tempfile.TemporaryDirectory() as d:
   with self.assertRaisesRegex(p.MemoryContractError,"MemoryImportResult"):p.run(broken,"run_x",lambda *_:None,d)
if __name__=="__main__":unittest.main()
