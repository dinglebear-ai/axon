import { Bot, RefreshCw, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { Client } from "@/lib/axonClient";
import {
  approveCodexOperation,
  CODEX_MUTATIONS,
  executeCodexOperation,
  prepareCodexOperation,
  type CodexMutation,
  type CodexOperation,
  type CodexResource,
} from "@/lib/codexControl";
import { useCodexControl } from "@/lib/useCodexControl";

const resources: CodexResource[] = ["account", "models", "config", "mcp_servers", "plugins", "skills", "hooks", "apps"];
type MutationKind = keyof typeof CODEX_MUTATIONS;

export function CodexControlView({ client, onClose }: { client: Client | null; onClose: () => void }) {
  const { snapshot, error, loading, refresh } = useCodexControl(client, true);
  const [resource, setResource] = useState<CodexResource>("account");
  const [kind, setKind] = useState<MutationKind>("pluginInstall");
  const [parameters, setParameters] = useState("{}");
  const [operation, setOperation] = useState<CodexOperation | null>(null);
  const [capability, setCapability] = useState("");
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mutation = useMemo<CodexMutation | null>(() => {
    try {
      const params = JSON.parse(parameters) as Record<string, unknown>;
      return { ...CODEX_MUTATIONS[kind], params };
    } catch { return null; }
  }, [kind, parameters]);

  async function prepare() {
    if (!client || !mutation) return;
    await run(async () => { setOperation(await prepareCodexOperation(client, mutation)); setCapability(""); });
  }
  async function approve() {
    if (!client || !operation) return;
    await run(async () => setCapability(await approveCodexOperation(client, operation.id)));
  }
  async function execute() {
    if (!client || !operation || !capability || !mutation) return;
    await run(async () => {
      await executeCodexOperation(client, operation.id, capability, mutation);
      setOperation({ ...operation, phase: "reconciled" });
      setCapability("");
      await refresh();
    });
  }
  async function run(action: () => Promise<void>) {
    setBusy(true); setMutationError(null);
    try { await action(); } catch (cause) { setMutationError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setBusy(false); }
  }

  return <main className="codex-control" aria-label="Codex app-server control">
    <header className="codex-control-header">
      <div><span className="settings-eyebrow">Axon server</span><h2><Bot size={20} /> Codex app-server</h2><p>{snapshot?.status.home ?? "Dedicated isolated control home"}</p></div>
      <div className="codex-control-actions"><Button variant="plain" onClick={() => void refresh()} disabled={loading}><RefreshCw size={15} /> Refresh</Button><Button variant="plain" onClick={onClose} aria-label="Close Codex control"><X size={16} /></Button></div>
    </header>
    {(error || mutationError) && <p className="settings-error" role="alert">{error ?? mutationError}</p>}
    <section className="codex-status-card"><strong>{snapshot?.status.state ?? (loading ? "starting" : "unavailable")}</strong><span>{snapshot?.status.detail ?? "Mutations require policy and a single-use human approval."}</span></section>
    <nav className="codex-tabs" aria-label="Codex resources">{resources.map(item => <Button key={item} variant="plain" className={resource === item ? "codex-tab-active" : ""} onClick={() => setResource(item)}>{item.replace("_", " ")}</Button>)}</nav>
    <section className="codex-resource"><h3>{resource.replace("_", " ")}</h3><pre>{JSON.stringify(snapshot?.[resource] ?? null, null, 2)}</pre></section>
    <section className="codex-mutation"><h3>Approved change</h3><p>Prepare the exact request, approve its digest, then execute the single-use capability.</p>
      <label>Workflow<select value={kind} onChange={event => { setKind(event.target.value as MutationKind); setOperation(null); setCapability(""); }}>
        <option value="config">Write config / add, edit, or remove MCP definition</option><option value="mcpReload">Reload MCP servers</option><option value="mcpOauth">Start MCP OAuth</option><option value="pluginInstall">Install plugin</option><option value="pluginUninstall">Uninstall plugin</option><option value="marketplaceAdd">Add marketplace</option><option value="marketplaceRemove">Remove marketplace</option><option value="marketplaceUpgrade">Upgrade marketplace</option><option value="skillConfig">Enable, disable, or configure skill</option><option value="skillImport">Import standalone skill or agent config</option>
      </select></label>
      <label>JSON parameters<textarea rows={7} value={parameters} onChange={event => setParameters(event.target.value)} aria-invalid={!mutation} /></label>
      <div className="codex-control-actions"><Button disabled={busy || !mutation || !client} onClick={() => void prepare()}>1 Prepare</Button><Button disabled={busy || !operation} onClick={() => void approve()}>2 Approve</Button><Button disabled={busy || !capability} onClick={() => void execute()}>3 Execute</Button></div>
      {operation && <pre>{JSON.stringify(operation, null, 2)}</pre>}
    </section>
  </main>;
}
