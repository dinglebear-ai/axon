#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import http.server
import json
import os
import signal
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[4]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


execute = load("retrieval_execute_tests", Path(__file__).with_name("execute.py"))
INTEGRATION_TIMEOUT = 15


FAKE_AXON = r'''#!/usr/bin/env python3
import json, os, pathlib, socket, sys, urllib.error, urllib.request
args = sys.argv[1:]
with open(os.environ["FAKE_AXON_CALLS"], "a", encoding="utf-8") as stream:
    stream.write(json.dumps({"argv": args, "data_dir": os.environ.get("AXON_DATA_DIR")}) + "\n")
command = args[0]
if len(args)>1 and args[1] in {"fixture provider classification probe","fixture dimension probe"}:
    expected={"fixture dimension probe":"provider.dimension_mismatch"}
    base=os.environ.get("TEI_URL") if args[1].startswith("fixture dimension") else os.environ.get("AXON_OPENAI_BASE_URL")
    url=base+('/embed' if args[1].startswith('fixture dimension') else '/chat/completions')
    try:
      req=urllib.request.Request(url,data=json.dumps({"inputs":["x"],"messages":[]}).encode(),headers={"Content-Type":"application/json"})
      with urllib.request.urlopen(req,timeout=1.2) as response: value=json.loads(response.read())
      if args[1].startswith('fixture dimension'): code='embedding.tei.dimension_mismatch' if len(value[0])!=8 else ''
      else: code='provider.schema_mismatch' if not value.get('choices') else ''
    except (TimeoutError,socket.timeout): code='provider.timeout'
    except urllib.error.HTTPError as error:
      try:
       value=json.loads(error.read()); upstream=value.get('error',{}).get('code','')
       code={"context_length_exceeded":"provider.token_limit"}.get(upstream,upstream)
      finally:error.close()
    except Exception: code='provider.malformed_response'
    print(json.dumps({"error":{"code":code},"code":code})); raise SystemExit()
if command == "doctor":
    print(json.dumps({"all_ok": os.environ.get("FAKE_DOCTOR_FAIL") != "1"})); raise SystemExit()
if command.endswith("documents") or command.endswith("empty-corpus"):
    print(json.dumps({"status":"completed","job_id":"job-index","source_id":"source-index"})); raise SystemExit()
if command == "ask" and "--list-sessions" in args:
    print("[]"); raise SystemExit()
if command == "ask" and "Transient retry probe" in args[1]:
    url=os.environ['AXON_OPENAI_BASE_URL']+'/chat/completions'; body=json.dumps({"messages":[]}).encode()
    for attempt in range(2):
      try:
        with urllib.request.urlopen(urllib.request.Request(url,data=body,headers={"Content-Type":"application/json"}),timeout=2) as response: value=json.loads(response.read())
        print(json.dumps({"answer":value['choices'][0]['message']['content']})); raise SystemExit()
      except urllib.error.HTTPError:
        if attempt: raise
if command == "ask" and "--resume" in args:
    marker=pathlib.Path(os.environ['AXON_DATA_DIR'])/'fake-session-value'
    if not marker.exists(): print(json.dumps({"answer":"unknown"})); raise SystemExit()
    print(json.dumps({"answer":marker.read_text(),"session":args[args.index('--resume')+1],"citations":[]})); raise SystemExit()
if command == "query" and ("deterministic-no-match" in args[1] or args[1] == "empty-collection-probe"):
    print("[]"); raise SystemExit()
if os.environ.get("FAKE_PROVIDER_ERROR") == command:
    print(json.dumps({"code":"provider.unavailable"})); raise SystemExit()
if command in {'ask','chat','summarize','research','evaluate','train'} and os.environ.get('AXON_OPENAI_BASE_URL') and not (command=='research' and 'fallback injection probe' in args[1]):
    count=3 if command=='evaluate' else 1
    for _ in range(count):
      req=urllib.request.Request(os.environ['AXON_OPENAI_BASE_URL']+'/chat/completions',data=json.dumps({"messages":[]}).encode(),headers={"Content-Type":"application/json"})
      with urllib.request.urlopen(req,timeout=2) as response: json.loads(response.read())
if command == "retrieve":
    u=str(pathlib.Path(os.environ['AXON_E2E_CORPUS_ROOT'])/'micro/unicode-東京-🧪.txt'); print(json.dumps({"chunk_count":1,"content":"The fixture city is 東京","requested_url":u,"matched_url":u,"truncated":False,"warnings":[],"variant_errors":[]})); raise SystemExit()
if command == "code-search":
    u=str(pathlib.Path(os.environ['AXON_E2E_CORPUS_ROOT'])/'representative/.fixture-note'); print(json.dumps({"results":[{"content":"DOTFILE-FACT is violet-echo","snippet":"DOTFILE-FACT: the synthetic audit code is violet-echo.","citation":{"source_id":"source.dotfile","chunk_id":"chunk-dotfile","canonical_uri":u}}]})); raise SystemExit()
u=str(pathlib.Path(os.environ['AXON_E2E_CORPUS_ROOT'])/'micro/atlas-v1.md'); payload = {"answer":"The Atlas beacon emits an amber signal.","snippet":"Atlas beacon emits an amber signal","citations":[{"source_id":"source.atlas","chunk_id":"chunk-atlas","canonical_uri":u}]}
if command=='query': payload={"results":[{"content":"The Atlas beacon emits an amber signal.","citation":{"source_id":"source.atlas","chunk_id":"chunk-atlas","canonical_uri":u}}]}
if command=='ask': payload['timing_ms']={"retrieval":2,"context_build":3,"llm":4,"total":9}
elif command=='evaluate': payload['timing_ms']={"retrieval":2,"context_build":3,"rag_llm":4,"baseline_llm":4,"research_elapsed_ms":0,"analysis_llm_ms":2,"total":15}
elif command=='summarize': payload['timing_ms']={"scrape":2,"llm":4,"total":6}
elif command=='research': payload['timing_ms']={"total":6}
if command in {'summarize','research'}: payload['usage']={"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}
if command=='ask' and '--session' in args:
    payload['session']=args[args.index('--session')+1]
    (pathlib.Path(os.environ['AXON_DATA_DIR'])/'fake-session-value').write_text('amber')
if command=='search': payload['results']=[{"url":u,"snippet":"Atlas beacon emits amber"}]
elif command=='chat': payload.update(reply=payload['answer'],session_id=os.environ['AXON_E2E_RUN_ID']+'_chat')
elif command=='summarize': payload['urls']=[u]; payload['documents']=[{"url":u,"content_chars":45}]; payload['summary']=payload['answer']; payload['context_chars']=45; payload['context_truncated']=False
elif command=='research': payload.update(query=args[1],limit=5,offset=0,search_results=[{"position":1,"title":"Atlas","url":u,"snippet":"amber"}],extractions=[{"url":u,"title":"Atlas","extracted":"Atlas beacon emits amber","source_type":"reference_docs","source_reputation":"high","instruction_trust":"evidence_only"}],source_index_status='completed',source_jobs=[],source_jobs_rejected=[],summary=payload['answer'],summary_source='llm')
elif command=='extract': payload.update(job_id='job-extract',status='completed',results=[{"url":u,"value":"amber"}])
elif command=='train': payload.update(event_id='event-train',candidates=[])
elif command=='suggest': payload['suggestions']=[]
if command=='ask': payload.update(query=args[1],warnings=[],diagnostics=None,explain=None)
elif command=='evaluate': payload.update(query=args[1],rag_answer=payload.pop('answer'),baseline_answer='amber',analysis_answer='supported',source_urls=[u],crawl_suggestions=[],crawl_enqueue_outcomes=[],ref_chunk_count=1,diagnostics=None)
elif command=='chat': payload={"session_id":payload['session_id'],"reply":payload['reply'],"model":"fixture"}
elif command=='search': payload={"results":payload['results']}
elif command=='summarize': payload={key:payload[key] for key in ('urls','documents','summary','context_chars','context_truncated','usage','timing_ms')}
elif command=='research': payload={"payload":{key:payload[key] for key in ('query','limit','offset','search_results','extractions','source_index_status','source_jobs','source_jobs_rejected','summary','summary_source','usage','timing_ms')}}
elif command=='extract': payload={"job_id":payload['job_id'],"status":payload['status'],"results":payload['results']}
elif command=='train': payload={"event_id":payload['event_id'],"candidates":payload['candidates']}
elif command=='suggest': payload={"suggestions":payload['suggestions']}
if os.environ.get("FAKE_FALLBACK") == "1" and command == "ask" and "configured fallback" in args[1]: payload["fallback_used"] = True
if command=='research' and 'fallback injection probe' in args[1]: payload['summary_source']='fallback'
print(json.dumps(payload))
'''


class ExecuteTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.binary = self.root / "axon"
        self.binary.write_text(FAKE_AXON, encoding="utf-8")
        self.binary.chmod(0o755)
        self.calls = self.root / "calls.jsonl"
        self.env = mock.patch.dict(os.environ, {"FAKE_AXON_CALLS": str(self.calls)}, clear=False)
        self.env.start()

    def tearDown(self):
        self.env.stop()
        self.temp.cleanup()

    def test_real_process_orchestration_indexes_corpus_and_runs_every_operation(self):
        evidence = execute.execute(self.binary, self.root / "out", timeout=INTEGRATION_TIMEOUT, require_all_surfaces=False)
        self.assertEqual(62, len(evidence))
        self.assertTrue(all(item["result"] == "pass" for item in evidence))
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        commands = [item["argv"][0] for item in calls]
        for operation in {"query", "retrieve", "search", "code-search", "ask", "chat",
                          "summarize", "research", "extract", "evaluate", "train", "suggest"}:
            self.assertIn(operation, commands)
        self.assertTrue(any(command.endswith("documents") for command in commands))
        data_dirs = {item["data_dir"] for item in calls if item["data_dir"]}
        self.assertEqual(2, len(data_dirs), "cross-run chat check must use a fresh AXON_DATA_DIR")

    def test_provider_preflight_is_mandatory(self):
        with mock.patch.dict(os.environ, {"FAKE_DOCTOR_FAIL": "1"}):
            with self.assertRaisesRegex(execute.ExecutionError, "preflight failed"):
                execute.execute(self.binary, self.root / "out", timeout=INTEGRATION_TIMEOUT, require_all_surfaces=False)
        calls = [json.loads(line)["argv"][0] for line in self.calls.read_text().splitlines()]
        self.assertEqual(["doctor"], calls)

    @unittest.skipIf(os.name == "nt", "POSIX process-group ownership regression")
    def test_invoke_timeout_terminates_and_reaps_descendant_process_group(self):
        descendant = self.root / "descendant.pid"
        binary = self.root / "timeout-parent"
        binary.write_text(
            "#!/usr/bin/env python3\nimport os,pathlib,subprocess,sys,time\n"
            "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])\n"
            "pathlib.Path(os.environ['DESCENDANT_PID']).write_text(str(child.pid))\n"
            "time.sleep(60)\n", encoding="utf-8")
        binary.chmod(0o755)
        env = {**os.environ, "DESCENDANT_PID": str(descendant)}
        with self.assertRaisesRegex(execute.ExecutionError, "timed out"):
            execute.invoke(binary, ["doctor"], env, 10)
        self.assertTrue(descendant.exists(), "timeout fixture never reached descendant creation")
        child_pid = int(descendant.read_text())
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            try: os.kill(child_pid, 0)
            except ProcessLookupError: break
            time.sleep(.02)
        else:self.fail("timed-out descendant remained alive after process-group teardown")

    def test_provider_error_from_real_command_never_becomes_pass(self):
        with mock.patch.dict(os.environ, {"FAKE_PROVIDER_ERROR": "query"}):
            with self.assertRaisesRegex(execute.ExecutionError, "semantic invariant failed"):
                execute.execute(self.binary, self.root / "out", timeout=INTEGRATION_TIMEOUT, require_all_surfaces=False)

    def test_configured_fallback_must_be_observed_in_actual_axon_output(self):
        with mock.patch.dict(os.environ, {"AXON_E2E_EXPECT_FALLBACK": "1", "FAKE_FALLBACK": "1"}):
            evidence = execute.execute(self.binary, self.root / "out", timeout=INTEGRATION_TIMEOUT, require_all_surfaces=False)
        self.assertEqual(62, len(evidence))

    def test_all_fixture_provider_modes_have_exact_structured_classification(self):
        evidence = execute.execute(self.binary, self.root / "out", timeout=INTEGRATION_TIMEOUT,
                                   fixture_provider_modes=True, require_all_surfaces=False)
        self.assertEqual(62, len(evidence))

    def test_http_and_mcp_paths_execute_shared_structured_clients(self):
        payload = {
            "answer": "The Atlas beacon emits an amber signal.",
            "snippet": "Atlas beacon emits an amber signal",
            "citations": [{"source_id": "source.atlas", "chunk_id": "chunk-atlas",
                           "canonical_uri": str(execute.ATLAS)}],
        }

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(inner):
                if inner.path != "/v1/jobs/job-http-extract": inner.send_error(404); return
                body = json.dumps({"job_id":"job-http-extract","status":"completed","results":[{"url":str(execute.ATLAS),"value":"amber"}]}).encode()
                inner.send_response(200); inner.send_header("Content-Type", "application/json")
                inner.send_header("Content-Length", str(len(body))); inner.end_headers(); inner.wfile.write(body)
            def do_POST(inner):
                required = {"/v1/query":{"query","collection","limit"},"/v1/retrieve":{"url","collection","max_points"},"/v1/search":{"query","limit"},"/v1/code-search":{"inputs","options"},"/v1/ask":{"query","collection","diagnostics"},"/v1/chat":{"message"},"/v1/summarize":{"urls"},"/v1/research":{"query","limit"},"/v1/extract":{"urls","prompt","embed"},"/v1/evaluate":{"question","collection","diagnostics"},"/v1/suggest":{"focus","collection"}}
                length = int(inner.headers.get("Content-Length", "0"))
                request = json.loads(inner.rfile.read(length) or b"{}")
                if inner.path not in required or set(request) != required[inner.path]:
                    inner.send_error(400, "request DTO mismatch"); return
                response = dict(payload)
                if inner.path == "/v1/retrieve":
                    response.update(answer="The fixture city is 東京", citations=[{
                        "source_id": "source.unicode", "chunk_id": "chunk-unicode",
                        "canonical_uri": str(execute.UNICODE)}], content="The fixture city is 東京",
                                    matched_url=str(execute.UNICODE))
                elif inner.path == "/v1/code-search":
                    response.update(answer="DOTFILE-FACT is violet-echo", citations=[{
                        "source_id": "source.dotfile", "chunk_id": "chunk-dotfile",
                        "canonical_uri": str(execute.FACT_DOCUMENTS["fact.dotfile.code"])}])
                if inner.path == "/v1/ask": response["timing_ms"] = {"retrieval":2,"context_build":3,"llm":4,"total":9}
                elif inner.path == "/v1/evaluate": response["timing_ms"] = {"retrieval":2,"context_build":3,"rag_llm":4,"baseline_llm":4,"research_elapsed_ms":0,"analysis_llm_ms":2,"total":15}
                elif inner.path == "/v1/summarize": response["timing_ms"] = {"scrape":2,"llm":4,"total":6}
                elif inner.path == "/v1/research": response["timing_ms"] = {"total":6}
                if inner.path in {"/v1/summarize", "/v1/research"}: response["usage"] = {"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}
                if inner.path == "/v1/search": response["results"] = [{"url":str(execute.ATLAS),"snippet":"Atlas beacon emits amber"}]
                elif inner.path == "/v1/chat": response.update(answer=response["answer"])
                elif inner.path == "/v1/summarize": response.update(urls=[str(execute.ATLAS)],summary=response["answer"],documents=[{"url":str(execute.ATLAS),"content_chars":45}],context_chars=45,context_truncated=False)
                elif inner.path == "/v1/research": response.update(payload={"query":"atlas","limit":5,"offset":0,"search_results":[{"position":1,"title":"Atlas","url":str(execute.ATLAS),"snippet":"amber"}],"extractions":[{"url":str(execute.ATLAS),"title":"Atlas","extracted":"Atlas beacon emits amber","source_type":"reference_docs","source_reputation":"high","instruction_trust":"evidence_only"}],"source_index_status":"completed","source_jobs":[],"source_jobs_rejected":[],"summary":response["answer"],"summary_source":"llm","usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30},"timing_ms":{"total":6}})
                elif inner.path == "/v1/extract": response = {"job_id":"job-http-extract","status":"accepted","status_url":"/v1/jobs/job-http-extract"}
                elif inner.path == "/v1/suggest": response["suggestions"] = []
                if inner.path == "/v1/query": response = {"results":[{"content":"The Atlas beacon emits amber","citation":payload["citations"][0]}]}
                elif inner.path == "/v1/code-search": response = {"results":[{"content":"DOTFILE-FACT is violet-echo","citation":response["citations"][0]}]}
                elif inner.path == "/v1/ask": response = {"query":"atlas","answer":response["answer"],"citations":response["citations"],"warnings":[],"diagnostics":None,"explain":None,"timing_ms":response["timing_ms"]}
                elif inner.path == "/v1/chat": response = {"message":"atlas","answer":response["answer"],"model":"fixture"}
                elif inner.path == "/v1/evaluate": response = {"query":"atlas","rag_answer":response["answer"],"baseline_answer":"amber","analysis_answer":"supported","citations":response["citations"],"source_urls":[str(execute.ATLAS)],"crawl_suggestions":[],"crawl_enqueue_outcomes":[],"ref_chunk_count":1,"diagnostics":None,"timing_ms":response["timing_ms"]}
                elif inner.path == "/v1/suggest": response = {"suggestions":response["suggestions"]}
                body = json.dumps(response).encode()
                inner.send_response(200); inner.send_header("Content-Type", "application/json")
                inner.send_header("Content-Length", str(len(body))); inner.end_headers(); inner.wfile.write(body)
            def log_message(inner, *_args): pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        self.addCleanup(server.server_close); self.addCleanup(server.shutdown)

        mcporter = self.root / "mcporter"
        mcporter.write_text(
            "#!/usr/bin/env python3\nimport json,sys\n"
            "a=json.loads(sys.argv[sys.argv.index('--args')+1]); op=a['action'].replace('_','-')\n"
            f"\nif op=='jobs': print(json.dumps({{'job':{{'job_id':'job-mcp-extract','status':'completed','results':[{{'url':{str(execute.ATLAS)!r},'value':'amber'}}]}}}})); raise SystemExit()\n"
            "b={k:v for k,v in a.items() if k not in {'action','subaction'}}; required={'query':{'query','collection','limit'},'retrieve':{'url','collection','max_points'},'search':{'query','limit'},'code-search':{'inputs','options'},'ask':{'query','collection','diagnostics'},'chat':{'message','session_id'},'summarize':{'urls'},'research':{'query','limit'},'extract':{'urls','prompt','embed'},'evaluate':{'query','collection','diagnostics'},'suggest':{'focus','collection','limit'}}; assert set(b)==required[op],(op,b)\n"
            f"p={json.dumps(payload)!r}; p=json.loads(p)\n"
            f"\nif op=='retrieve': p.update(answer='The fixture city is 東京',content='The fixture city is 東京',matched_url={str(execute.UNICODE)!r},citations=[{{'source_id':'source.unicode','chunk_id':'chunk-unicode','canonical_uri':{str(execute.UNICODE)!r}}}])\n"
            f"elif op=='code-search': p.update(answer='DOTFILE-FACT is violet-echo',citations=[{{'source_id':'source.dotfile','chunk_id':'chunk-dotfile','canonical_uri':{str(execute.FACT_DOCUMENTS['fact.dotfile.code'])!r}}}])\n"
            "\nif op=='ask': p['timing_ms']={'retrieval':2,'context_build':3,'llm':4,'total':9}\n"
            "elif op=='evaluate': p['timing_ms']={'retrieval':2,'context_build':3,'rag_llm':4,'baseline_llm':4,'research_elapsed_ms':0,'analysis_llm_ms':2,'total':15}\n"
            "elif op=='summarize': p['timing_ms']={'scrape':2,'llm':4,'total':6}\n"
            "elif op=='research': p['timing_ms']={'total':6}\n"
            "\nif op in {'summarize','research'}: p['usage']={'prompt_tokens':20,'completion_tokens':10,'total_tokens':30}\n"
            f"\nif op=='search': p['results']=[{{'url':{str(execute.ATLAS)!r},'snippet':'Atlas beacon emits amber'}}]\n"
            "elif op=='chat': p.update(answer=p['answer'],session_id=b['session_id'])\n"
            f"elif op=='summarize': p.update(urls=[{str(execute.ATLAS)!r}],summary=p['answer'],documents=[{{'url':{str(execute.ATLAS)!r},'content_chars':45}}],context_chars=45,context_truncated=False)\n"
            f"elif op=='research': p={{'payload':{{'query':'atlas','limit':5,'offset':0,'search_results':[{{'position':1,'title':'Atlas','url':{str(execute.ATLAS)!r},'snippet':'amber'}}],'extractions':[{{'url':{str(execute.ATLAS)!r},'title':'Atlas','extracted':'Atlas beacon emits amber','source_type':'reference_docs','source_reputation':'high','instruction_trust':'evidence_only'}}],'source_index_status':'completed','source_jobs':[],'source_jobs_rejected':[],'summary':'amber','summary_source':'llm','usage':{{'prompt_tokens':20,'completion_tokens':10,'total_tokens':30}},'timing_ms':{{'total':6}}}}}}\n"
            "elif op=='extract': p={'job_id':'job-mcp-extract','status':'accepted'}\n"
            "elif op=='suggest': p['suggestions']=[]\n"
            "\nif op=='query': p={'results':[{'content':'The Atlas beacon emits amber','citation':p['citations'][0]}]}\n"
            "elif op=='code-search': p={'results':[{'content':'DOTFILE-FACT is violet-echo','citation':p['citations'][0]}]}\n"
            "elif op=='ask': p={'query':'atlas','answer':p['answer'],'citations':p['citations'],'warnings':[],'diagnostics':None,'explain':None,'timing_ms':p['timing_ms']}\n"
            "elif op=='chat': p={'session_id':b['session_id'],'reply':p['answer'],'model':'fixture'}\n"
            "elif op=='evaluate': p={'query':'atlas','rag_answer':p['answer'],'baseline_answer':'amber','analysis_answer':'supported','citations':p['citations'],'source_urls':[],'crawl_suggestions':[],'crawl_enqueue_outcomes':[],'ref_chunk_count':1,'diagnostics':None,'timing_ms':p['timing_ms']}\n"
            "elif op=='search': p={'results':p['results']}\n"
            "elif op=='summarize': p={k:p[k] for k in ('urls','documents','summary','context_chars','context_truncated','usage','timing_ms')}\n"
            "elif op=='suggest': p={'suggestions':p['suggestions']}\n"
            "print(json.dumps(p))\n", encoding="utf-8")
        mcporter.chmod(0o755)
        # This integration case starts three transport surfaces and dozens of
        # short-lived interpreters; leave scheduler headroom on loaded CI hosts.
        # The dedicated timeout regression above retains a 200 ms hard limit.
        evidence = execute.execute(
            self.binary, self.root / "transport-out", timeout=15,
            http_url=f"http://127.0.0.1:{server.server_port}", mcporter=mcporter,
        )
        self.assertEqual(173, len(evidence))
        self.assertEqual({"cli", "http", "mcp", "harness"}, {item["surface"] for item in evidence})

    def test_missing_explicit_provider_and_timing_evidence_fails_closed(self):
        actual = {"answer": "amber", "source_id": "source.atlas",
                  "citations": [{"id": "cite:atlas-v1:beacon", "source_id": "source.atlas",
                                 "excerpt": "Atlas beacon emits an amber signal"}]}
        item = execute.scenarios()[0]
        normalized = execute.normalize(actual, item, "query", "axon_e2e_fixture", 9)
        self.assertIsNone(normalized["provider_usage"])
        ask = next(value for value in execute.scenarios() if value["operation"] == "ask")
        normalized["operation"] = "ask"
        result = execute.grounding.evaluate(ask, normalized, execute.SEMANTICS,
                                            run_id="axon_e2e_fixture")
        self.assertFalse(next(assertion["passed"] for assertion in result["assertions"]
                              if assertion["id"] == "timing.public_fields"))

    def test_unavailable_binary_fails_before_allocating_or_falling_back(self):
        with self.assertRaisesRegex(execute.ExecutionError, "unavailable"):
            execute.execute(self.root / "missing-axon", self.root / "out", timeout=INTEGRATION_TIMEOUT)
        self.assertFalse(self.calls.exists())

    def test_actual_jsonl_is_parsed_but_malformed_output_fails(self):
        self.assertEqual([{"rank": 1}, {"rank": 2}], execute.parse_output(b'{"rank":1}\n{"rank":2}\n'))
        with self.assertRaisesRegex(execute.ExecutionError, "neither JSON nor JSONL"):
            execute.parse_output(b'{"rank":1}\nnot-json\n')

    def test_duplicate_actual_artifact_ids_remain_visible_to_multiplication_checks(self):
        actual = {"artifact_id": "art_duplicate", "artifacts": ["art_duplicate"]}
        identities = execute.actual_artifact_ids(actual)
        self.assertEqual(["art_duplicate", "art_duplicate"], identities)
        self.assertNotEqual(len(identities), len(set(identities)))

    def test_failed_retry_artifact_check_fails_the_run(self):
        checks = [{"id": "provider.retry_artifacts_not_multiplied", "passed": False,
                   "detail": "duplicate actual artifact identity"}]
        with self.assertRaisesRegex(execute.ExecutionError, "retry_artifacts_not_multiplied"):
            execute.require_passing_checks(checks, "transient retry provider observation")


if __name__ == "__main__":
    unittest.main()
