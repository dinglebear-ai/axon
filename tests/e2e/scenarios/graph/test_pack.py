import importlib.util, unittest
from pathlib import Path
S=importlib.util.spec_from_file_location("graph_pack",Path(__file__).with_name("pack.py")); p=importlib.util.module_from_spec(S); S.loader.exec_module(p)
class Tests(unittest.TestCase):
 def test_exact_graph(self):
  def cli(*a,**k):
   if a[1]=="kinds": return {"kinds":["source"]}
   if a[1]=="query": return {"nodes":[{"node_id":"n1","source_id":"src1","uri":"uri1"},{"node_id":"n2"}],"edges":[{"edge_id":"e1","from_node_id":"n1","to_node_id":"n2","metadata":{"conflict_ids":["c1"]},"evidence":[{"evidence_id":"v1","source_id":"src1"}]}],"evidence":[{"evidence_id":"v1","source_id":"src1"}],"warnings":[]}
   if a[1]=="node": return {"node":{"node_id":a[2]},"edges":[]}
   if a[1]=="edge": return {"edge_id":a[2]}
   if a[1]=="resolve": return {"resolved":[{"node":{"node_id":"n1"}}],"misses":[],"warnings":[]}
   return {"source_id":"src1","canonical_uri":"uri1"}
  expected={"node_ids":["n1","n2"],"edge_id":"e1","evidence_id":"v1","conflict_id":"c1"}
  self.assertEqual("n1",p.run(cli,"src1","uri1",expected)["nodes"][0]["node_id"])
 def test_negative(self): self.assertEqual(2,len(p.negative(lambda *a,**k:{"code":"graph.not_found"})))
if __name__=="__main__": unittest.main()
