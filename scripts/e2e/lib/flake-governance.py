#!/usr/bin/env python3
"""Fail-closed E2E retry, quarantine, and rolling reliability governance."""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import random
import importlib.util
import sys
import time
from collections import defaultdict
from pathlib import Path
from typing import Any

REQUIRED = {"scenario_id","owner","rationale","issue","tier","environment","created_on","expires_on","restoration_criteria"}
PROTECTED_WORDS = {"security","cleanup","redaction","secret","trust","auth","ssrf","teardown"}
MASK_FORBIDDEN = {"product","auth_network","cleanup"}
SAFE_RETRY_CLASSES = {"provider","fixture","harness"}
UNSAFE_LIFECYCLES = {"destructive","migration","upload","watch"}
PROVIDER_HEALTH_FIELDS = {"schema","kind","provider","passed","classification"}


class GovernanceError(RuntimeError): pass


def _reporting():
    spec=importlib.util.spec_from_file_location("axon_e2e_governance_reporting",Path(__file__).with_name("reporting.py"))
    if spec is None or spec.loader is None:raise GovernanceError("canonical reporting unavailable")
    module=importlib.util.module_from_spec(spec);sys.modules[spec.name]=module;spec.loader.exec_module(module);return module


reporting=_reporting()


def _date(value: Any, field: str) -> dt.date:
    try:return dt.date.fromisoformat(value)
    except (TypeError,ValueError) as error:raise GovernanceError(f"quarantine {field} must be an ISO date") from error


def protected(scenario: dict[str,Any]) -> bool:
    values=[scenario.get("id",""),scenario.get("capability",""),scenario.get("lifecycle",""),*scenario.get("tags",[]),*scenario.get("semantic_oracles",[])]
    return any(word in str(value).lower() for value in values for word in PROTECTED_WORDS)


def provider_health_failure(item:Any)->bool:
    """Only this exact typed observation is non-product provider evidence."""
    return (isinstance(item,dict) and set(item)==PROVIDER_HEALTH_FIELDS and item.get("schema")==1
            and item.get("kind")=="provider_health" and isinstance(item.get("provider"),str)
            and bool(item["provider"].strip()) and item.get("passed") is False
            and item.get("classification")=="provider")


def validate_quarantines(document: dict[str,Any], catalog: dict[str,Any], *, today: dt.date|None=None) -> dict[str,dict]:
    if set(document)!={"schema","quarantines"} or document.get("schema")!=1 or not isinstance(document.get("quarantines"),list):
        raise GovernanceError("quarantine document schema changed")
    scenarios={item["id"]:item for item in catalog.get("scenarios",[])};active={};today=today or dt.date.today()
    for index,item in enumerate(document["quarantines"]):
        if not isinstance(item,dict) or set(item)!=REQUIRED:raise GovernanceError(f"quarantine {index} must contain exactly {sorted(REQUIRED)}")
        if any(not isinstance(item[field],str) or not item[field].strip() for field in REQUIRED):raise GovernanceError(f"quarantine {index} contains an empty field")
        if len(item["rationale"])<12 or len(item["restoration_criteria"])<12:raise GovernanceError("quarantine rationale/restoration criteria are not substantive")
        if not item["issue"].startswith("https://"):raise GovernanceError("quarantine issue must be a reviewable HTTPS link")
        if item["tier"] not in {"hermetic","live"}:raise GovernanceError("quarantine tier is invalid")
        created,expires=_date(item["created_on"],"created_on"),_date(item["expires_on"],"expires_on")
        if expires<=created:raise GovernanceError("quarantine expiry must follow creation")
        if expires<today:raise GovernanceError(f"expired quarantine: {item['scenario_id']}")
        scenario=scenarios.get(item["scenario_id"])
        if scenario is None:raise GovernanceError(f"quarantine references unknown scenario: {item['scenario_id']}")
        if protected(scenario):raise GovernanceError(f"protected scenario cannot be quarantined: {item['scenario_id']}")
        if item["scenario_id"] in active:raise GovernanceError(f"duplicate quarantine: {item['scenario_id']}")
        active[item["scenario_id"]]=item
    return active


def validate_attempts(report: dict[str,Any]) -> None:
    retry_count=0;namespaces=set();retry_policies=[]
    for scenario in report.get("scenarios",[]):
        attempts=scenario.get("attempts",[]);first=scenario.get("first_attempt_failure")
        if not attempts:raise GovernanceError(f"missing attempt evidence: {scenario.get('scenario_id')}")
        expected=next((item for item in attempts if item.get("status")!="passed"),None)
        if first!=expected:raise GovernanceError(f"first-attempt failure evidence changed: {scenario['scenario_id']}")
        if len(attempts)>1:
            retry_count+=1
            if scenario.get("tier")!="live":raise GovernanceError(f"hermetic scenario retried: {scenario['scenario_id']}")
            if len(attempts)>2:raise GovernanceError(f"diagnostic retry budget exceeded: {scenario['scenario_id']}")
            policies=[item["retry_policy"] for item in scenario.get("invariants",[]) if isinstance(item,dict) and isinstance(item.get("retry_policy"),dict)]
            if len(policies)!=1:raise GovernanceError(f"retry policy evidence missing: {scenario['scenario_id']}")
            policy=policies[0]
            required={"declared","fresh_namespace","serialized","backoff_ms","suite_budget_declared","budget_before","suite_budget_remaining","retry_ordinal","teardown_verified","retry_safe","previous_attempt_namespace","attempt_namespace"}
            if set(policy)!=required or policy["declared"] is not True or policy["fresh_namespace"] is not True or policy["serialized"] is not True:
                raise GovernanceError(f"retry policy evidence invalid: {scenario['scenario_id']}")
            if not isinstance(policy["backoff_ms"],int) or not 50<=policy["backoff_ms"]<=2000:
                raise GovernanceError(f"retry budget/backoff invalid: {scenario['scenario_id']}")
            if policy["retry_safe"] is not True or policy["teardown_verified"] is not True:
                raise GovernanceError(f"ambiguous mutation was retried: {scenario['scenario_id']}")
            attempt_names=(policy["previous_attempt_namespace"],policy["attempt_namespace"])
            if any(not isinstance(value,str) or not value.startswith("axon_e2e_") for value in attempt_names) or len(set(attempt_names))!=2:
                raise GovernanceError(f"fresh retry namespace evidence invalid: {scenario['scenario_id']}")
            if any(value in namespaces for value in attempt_names):raise GovernanceError("retry namespace was reused across scenarios")
            namespaces.update(attempt_names)
            if attempts[0].get("namespace")!=attempt_names[0] or attempts[1].get("namespace")!=attempt_names[1]:raise GovernanceError(f"attempt namespace evidence disagrees with retry policy: {scenario['scenario_id']}")
            if attempts[1].get("serialized") is not True or attempts[1].get("backoff_ms")!=policy["backoff_ms"] or attempts[1].get("teardown_verified") is not True:
                raise GovernanceError(f"attempt retry safety evidence disagrees with policy: {scenario['scenario_id']}")
            if attempts[0].get("classification") not in SAFE_RETRY_CLASSES:raise GovernanceError(f"non-diagnostic failure retried: {scenario['scenario_id']}")
            retry_policies.append(policy)
        summary=" ".join(str(item.get("summary") or "") for item in attempts).lower()
        if ("queue_expir" in summary or "circuit_breaker" in summary or "circuit breaker" in summary) and scenario.get("status")=="passed":
            raise GovernanceError(f"queue expiry/circuit breaker reported as pass: {scenario['scenario_id']}")
        substantive_failed=any(isinstance(item,dict) and item.get("passed") is False and not provider_health_failure(item) for item in scenario.get("invariants",[]))
        if substantive_failed and any(item.get("classification")=="provider" for item in attempts):raise GovernanceError(f"provider outage masked product assertion: {scenario['scenario_id']}")
    if retry_count:
        budget=report.get("policy",{}).get("suite_retry_budget")
        if not isinstance(budget,int) or budget<retry_count:raise GovernanceError("suite-wide diagnostic retry budget missing or exceeded")
        ordered=sorted(retry_policies,key=lambda item:item.get("retry_ordinal",-1))
        for index,policy in enumerate(ordered,1):
            expected_before=budget-index+1
            if (policy.get("retry_ordinal"),policy.get("suite_budget_declared"),policy.get("budget_before"),policy.get("suite_budget_remaining"))!=(index,budget,expected_before,expected_before-1):
                raise GovernanceError("suite-wide diagnostic retry budget evidence is forged or non-monotonic")


def retry_evidence(*,scenario_id:str,lifecycle:str,retry_class:str,tier:str,classification:str,budget_remaining:int,teardown_verified:bool,seed:str,suite_budget_declared:int|None=None,retry_ordinal:int=1) -> dict[str,Any]|None:
    if tier!="live" or retry_class not in {"diagnostic","provider_transient"} or classification not in SAFE_RETRY_CLASSES:return None
    if lifecycle in UNSAFE_LIFECYCLES and not teardown_verified:return None
    if budget_remaining<1:return None
    jitter=random.Random(hashlib.sha256(f"{seed}:{scenario_id}".encode()).digest()).randint(50,500)
    namespace=hashlib.sha256(f"{seed}:{scenario_id}".encode()).hexdigest()[:20]
    declared=budget_remaining if suite_budget_declared is None else suite_budget_declared
    return {"declared":True,"fresh_namespace":True,"serialized":True,"backoff_ms":jitter,
            "suite_budget_declared":declared,"budget_before":budget_remaining,"suite_budget_remaining":budget_remaining-1,"retry_ordinal":retry_ordinal,
            "teardown_verified":teardown_verified,"retry_safe":True,
            "previous_attempt_namespace":f"axon_e2e_{namespace}_attempt_1","attempt_namespace":f"axon_e2e_{namespace}_attempt_2"}


def run_live_diagnostic(*,scenario:Any,lifecycle:str,retry_class:str,budget_remaining:int,seed:str,invoke:Any,verify_teardown:Any,suite_budget_declared:int|None=None,retry_ordinal:int=1)->dict[str,Any]|None:
    """Execute the sole governed live retry path; callers provide the real invocation."""
    first_namespace=f"axon_e2e_{hashlib.sha256(f'{seed}:{scenario.scenario_id}'.encode()).hexdigest()[:20]}_attempt_1"
    started=time.monotonic();status,classification,summary=invoke(first_namespace)
    scenario.attempt(status,int((time.monotonic()-started)*1000),classification=classification,summary=summary,namespace=first_namespace)
    if status=="passed":return None
    teardown_ok=verify_teardown(first_namespace) is True
    policy=retry_evidence(scenario_id=scenario.scenario_id,lifecycle=lifecycle,retry_class=retry_class,tier=scenario.tier,
        classification=classification,budget_remaining=budget_remaining,teardown_verified=teardown_ok,seed=seed,
        suite_budget_declared=suite_budget_declared,retry_ordinal=retry_ordinal)
    if policy is None:return None
    time.sleep(policy["backoff_ms"]/1000)
    started=time.monotonic();status,classification,summary=invoke(policy["attempt_namespace"])
    scenario.attempt(status,int((time.monotonic()-started)*1000),classification=classification,summary=summary,
        namespace=policy["attempt_namespace"],serialized=True,backoff_ms=policy["backoff_ms"],teardown_verified=teardown_ok)
    scenario.invariants.append({"retry_policy":policy});return policy


def reliability(reports:list[dict[str,Any]],active:dict[str,dict],*,environment:str,window:int=20) -> dict[str,Any]:
    rows=defaultdict(list)
    for report in reports[-window:]:
        providers=json.dumps(report.get("provider_versions",{}),sort_keys=True,separators=(",",":"))
        for scenario in report.get("scenarios",[]):rows[(scenario["scenario_id"],scenario["tier"],environment,providers)].append(scenario)
    output=[];escalations=[]
    for (scenario_id,tier,env,providers),values in sorted(rows.items()):
        durations=sorted(sum(item.get("duration_ms",0) for item in value["attempts"]) for value in values)
        failures=sum(value["status"]!="passed" or value["attempts"][0]["status"]!="passed" for value in values)
        recovered=sum(value["status"]=="passed" and value["attempts"][0]["status"]!="passed" for value in values)
        entry=active.get(scenario_id);quarantined=bool(entry and entry["environment"]==env and entry["tier"]==tier)
        row={"scenario_id":scenario_id,"tier":tier,"environment":env,"provider_versions":json.loads(providers),
             "runs":len(values),"passes":len(values)-failures,"failures":failures,"diagnostic_recoveries":recovered,"pass_rate":round((len(values)-failures)/len(values),4),
             "runtime_ms":{"p50":durations[len(durations)//2],"p95":durations[min(len(durations)-1,max(0,(len(durations)*95+99)//100-1))]},
             "quarantined":quarantined,"healthy_coverage":not quarantined and failures==0}
        output.append(row)
        if len(values)>=5 and sum(value["status"]!="passed" or value["attempts"][0]["status"]!="passed" for value in values[-5:])>=3:
            issue=(entry or {}).get("issue");escalations.append({"scenario_id":scenario_id,"signal":"tracked_defect_required","issue":issue,"tracked":bool(issue)})
    denominator=sum(not row["quarantined"] for row in output);healthy=sum(row["healthy_coverage"] for row in output)
    return {"schema":1,"window":window,"segments":output,"escalations":escalations,
            "healthy_scenarios":healthy,"quarantined_scenarios":sum(row["quarantined"] for row in output),
            "coverage":{"observed":len(output),"denominator":denominator,"healthy":healthy,
                        "percent":round(100*healthy/denominator,2) if denominator else 0,
                        "quarantined_excluded_from_denominator":True}}


def validate_history(envelope:dict[str,Any],*,repository:str,workflow:str,trusted_ref:str)->list[dict[str,Any]]:
    required={"schema","repository","workflow","trusted_ref","reports"}
    if not isinstance(envelope,dict) or set(envelope)!=required or envelope.get("schema")!=1:raise GovernanceError("reliability history envelope is malformed")
    if (envelope.get("repository"),envelope.get("workflow"),envelope.get("trusted_ref"))!=(repository,workflow,trusted_ref):raise GovernanceError("reliability history provenance is untrusted")
    if not isinstance(envelope.get("reports"),list) or len(envelope["reports"])>20:raise GovernanceError("reliability history window is invalid")
    for report in envelope["reports"]:
        try:reporting.validate_report(report)
        except reporting.ReportingError as error:raise GovernanceError(f"reliability history contains invalid canonical report: {error}") from error
    return envelope["reports"]


def history_envelope(reports:list[dict[str,Any]],*,repository:str,workflow:str,trusted_ref:str)->dict[str,Any]:
    return {"schema":1,"repository":repository,"workflow":workflow,"trusted_ref":trusted_ref,"reports":reports[-20:]}


def govern(report:dict[str,Any],catalog:dict[str,Any],quarantine:dict[str,Any],*,environment:str,history:list[dict]|None=None,today:dt.date|None=None)->dict[str,Any]:
    try:reporting.validate_report(report)
    except reporting.ReportingError as error:raise GovernanceError(f"canonical report invalid: {error}") from error
    active=validate_quarantines(quarantine,catalog,today=today);validate_attempts(report)
    observed={item["scenario_id"] for item in report.get("scenarios",[])}
    for scenario_id,item in active.items():
        if item["environment"]==environment and not any(row.get("scenario_id")==scenario_id and row.get("tier")==item["tier"] for row in report.get("scenarios",[])):
            raise GovernanceError(f"quarantined scenario did not execute: {scenario_id}")
    for row in report.get("scenarios",[]):
        entry=active.get(row["scenario_id"])
        if not entry:continue
        attempts=row.get("attempts",[])
        if any(item.get("classification") in MASK_FORBIDDEN for item in attempts) or not row.get("cleanup",{}).get("success",False):raise GovernanceError(f"quarantine cannot mask required failure: {row['scenario_id']}")
    return reliability([*(history or []),report],active,environment=environment)
