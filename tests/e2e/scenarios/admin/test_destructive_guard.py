import importlib.util
import time
import unittest
import json
from pathlib import Path

SPEC=importlib.util.spec_from_file_location("guard",Path(__file__).with_name("destructive_guard.py"))
guard=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(guard)


class DestructiveGuardTests(unittest.TestCase):
    def setUp(self):
        self.run="axon_e2e_run_abc"; self.key=b"k"*32; self.expiry=int(time.time()*1000)+60_000
        self.targets=[{"type":"collection","identity":"axon_e2e_collection_a",
                       "ownership_marker":self.run+":marker"},
                      {"type":"upload","identity":"axon_e2e_upload_b",
                       "ownership_marker":self.run+":marker"}]
        self.plan=guard.plan_payload(self.run,2,self.targets,self.expiry)

    def test_digest_binds_run_attempt_targets_markers_and_expiry(self):
        base=guard.digest(self.plan,self.key)
        for key,value in (("run_id","axon_e2e_other"),("attempt",3),("expires_unix_ms",self.expiry+1)):
            changed=dict(self.plan); changed[key]=value
            self.assertNotEqual(base,guard.digest(changed,self.key))
        changed={**self.plan,"targets":[{**self.plan["targets"][0],"ownership_marker":self.run+":changed"}]}
        self.assertNotEqual(base,guard.digest(changed,self.key))

    def test_revalidates_immediately_before_each_delete(self):
        calls=0; deleted=[]
        def fetch():
            nonlocal calls
            calls+=1
            if calls == 3:
                return {**self.plan,"targets":self.plan["targets"][:-1]}
            return self.plan
        confirmation=guard.Confirmation(guard.digest(self.plan,self.key),self.run,2)
        with self.assertRaisesRegex(guard.GuardError,"changed"):
            guard.execute(fetch,confirmation,self.key,lambda target: deleted.append(target["identity"]))
        self.assertEqual(1,len(deleted))

    def test_mismatch_expiry_foreign_active_and_ambiguous_fail_closed(self):
        with self.assertRaises(guard.GuardError): guard.plan_payload(self.run,1,self.targets*2,self.expiry)
        with self.assertRaises(guard.GuardError): guard.plan_payload(self.run,1,[{**self.targets[0],"identity":"prod"}],self.expiry)
        confirmation=guard.Confirmation("0"*64,self.run,2)
        with self.assertRaisesRegex(guard.GuardError,"mismatch"):
            guard.execute(lambda:self.plan,confirmation,self.key,lambda _:None)
        valid=guard.Confirmation(guard.digest(self.plan,self.key),self.run,2)
        with self.assertRaisesRegex(guard.GuardError,"expired"):
            guard.execute(lambda:self.plan,valid,self.key,lambda _:None,lambda:self.expiry)

    def test_success_and_empty_repeat_are_idempotent(self):
        deleted=[]; confirmation=guard.Confirmation(guard.digest(self.plan,self.key),self.run,2)
        self.assertEqual([item["identity"] for item in self.plan["targets"]],
                         guard.execute(lambda:self.plan,confirmation,self.key,lambda item:deleted.append(item)))
        empty=guard.plan_payload(self.run,3,[],self.expiry)
        conf=guard.Confirmation(guard.digest(empty,self.key),self.run,3)
        self.assertEqual([],guard.execute(lambda:empty,conf,self.key,lambda _:self.fail("delete")))

    def test_admin_catalog_covers_every_destructive_family(self):
        catalog=json.loads(Path(__file__).with_name("scenarios.json").read_text())
        self.assertEqual({"prune","reset","migrate","cleanup","config","setup"},
                         {item["operation"] for item in catalog["scenarios"]})

    def test_admin_pack_has_real_composed_entry(self):
        text=Path(__file__).with_name("hermetic_entry.py").read_text()
        for contract in ("/v1/prune/plan", "/v1/prune/exec", '"reset"', '"migrate"',
                         "guard.execute", "teardown.py", "spawn_owned_process",
                         '"chunk_text":"owned migration ownership marker"'):
            self.assertIn(contract,text)

if __name__=="__main__": unittest.main()
