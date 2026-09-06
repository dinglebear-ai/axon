"""Watch transitions, due-tick linkage, and duplicate-scheduling pack."""
import concurrent.futures
class WatchContractError(RuntimeError): pass
def run(cli, source, namespace, register):
    created=cli("watch","create",source,"--every-seconds","60","--collection",namespace,"--json")
    watch_id=created.get("watch_id") or created.get("id")
    if not isinstance(watch_id,str): raise WatchContractError("watch create omitted ID")
    register("watch",watch_id)
    cli("watch","update",watch_id,"--every-seconds","120","--collection",namespace,"--json")
    updated=cli("watch","get",watch_id,"--json")
    request=updated.get("request",updated)
    if request.get("schedule",{}).get("every_seconds") != 120 or request.get("watch_id") != watch_id: raise WatchContractError(f"watch update did not persist: {updated}")
    cli("watch","pause",watch_id,"--json"); paused=cli("watch","status",watch_id,"--json")
    if paused.get("watch",{}).get("enabled") is not False: raise WatchContractError("watch pause state did not persist")
    cli("watch","resume",watch_id,"--json"); resumed=cli("watch","status",watch_id,"--json")
    if resumed.get("watch",{}).get("enabled") is not True: raise WatchContractError("watch resume state did not persist")
    def execute():
        try: return cli("watch","exec",watch_id,"--json")
        except RuntimeError as error: return {"error":str(error)}
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        runs=[future.result() for future in [pool.submit(execute) for _ in range(2)]]
    job_ids={value.get("job_id") or value.get("id") for value in runs if isinstance(value.get("job_id") or value.get("id"),str)}
    classified=all((isinstance(value.get("job_id") or value.get("id"),str) or "watch.execution_busy" in value.get("error","") or any(code in value.get("error","").lower() for code in ("busy","conflict","already","active"))) for value in runs)
    if len(job_ids)!=1 or not classified: raise WatchContractError(f"overlapping tick was not deduplicated/classified: {runs}")
    job_id=next(iter(job_ids)); register("job",job_id)
    history=cli("watch","history",watch_id,"--json"); job=cli("jobs","get",job_id,"--json")
    if watch_id not in str(history) or job_id not in str(history): raise WatchContractError("watch history lost tick/job linkage")
    summary=job.get("summary",job.get("job",job))
    if (summary.get("id") or summary.get("job_id")) != job_id or str(summary.get("kind",summary.get("task_kind",""))).lower() not in {"source","source_job"}: raise WatchContractError("watch tick did not link a canonical source job")
    if history.get("watch_id") != watch_id or not isinstance(history.get("jobs"),list): raise WatchContractError("watch history DTO drifted")
    for item in history["jobs"]:
        identity=item.get("id") if isinstance(item,dict) else None
        if not isinstance(identity,str): raise WatchContractError("watch history job omitted id")
        if identity != job_id: register("job",identity)
    cli("watch","delete",watch_id,"--json")
    missing=cli("watch","get",watch_id,"--json",ok=False)
    if not missing.get("code") and not missing.get("error"): raise WatchContractError("deleted watch remained visible")
    return {"watch_id":watch_id,"job_id":job_id,"history":history}
