import importlib.util, unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("task_wire", ROOT/"scripts/test-mcp-tasks-wire.py")
wire = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(wire)
class Fake:
    def __init__(self, responses): self.responses = iter(responses)
    def request(self, payload, timeout=30):
        response, notices = next(self.responses); response["id"] = payload["id"]; return response, notices
class TaskWireTests(unittest.TestCase):
    def test_structured_error(self):
        self.assertEqual(-32601, wire.structured_error({"error":{"code":-32601,"message":"missing"}}, "probe")["code"])
        with self.assertRaises(wire.WireError): wire.structured_error({"error":"missing"}, "probe")
    def test_progress_is_monotonic(self):
        values = [{"method":"notifications/progress","params":{"progress":n}} for n in (1,2)]
        self.assertEqual([1.0,2.0], wire.progress_values(values))
        with self.assertRaisesRegex(wire.WireError,"regressed"): wire.progress_values(list(reversed(values)))
    def test_poll_terminal_transition(self):
        states, detail, _ = wire.poll(Fake([({"result":{"status":"working"}},[]),
            ({"result":{"status":"completed","content":[]}},[])]), "extract:id", 1, attempts=2, delay=0)
        self.assertEqual(["working","completed"], states); self.assertEqual("completed", detail["status"])
    def test_poll_exhaustion_fails(self):
        with self.assertRaisesRegex(wire.WireError,"terminal"):
            wire.poll(Fake([({"result":{"status":"working"}},[])]), "extract:id", 1, attempts=1, delay=0)
    def test_create_requires_initial_progress(self):
        transport = Fake([({"result":{"taskId":"extract:id"}},[])])
        with self.assertRaisesRegex(wire.WireError,"initial progress"):
            wire.create(transport, 1, "https://example.com", "title", "token")
    def test_hostile_id_remains_data(self):
        value = "$(touch /tmp/nope);\nAuthorization: secret"
        self.assertEqual(value, wire.rpc(1,"tasks/get",{"taskId":value})["params"]["taskId"])
if __name__ == "__main__": unittest.main()
