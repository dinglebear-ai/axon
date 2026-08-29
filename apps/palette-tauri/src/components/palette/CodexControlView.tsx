import { Bot, RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { Client } from "@/lib/axonClient";
import {
  approveCodexOperation,
  buildConfigBatchMutation,
  buildMcpConfigMutation,
  CODEX_MUTATIONS,
  type CodexMutation,
  type CodexOperation,
  type CodexResource,
  executeCodexOperation,
  parseConfigValue,
  prepareCodexOperation,
  reconcileCodexOperation,
  respondToCodexServerRequest,
} from "@/lib/codexControl";
import { useCodexControl } from "@/lib/useCodexControl";

const resources: CodexResource[] = [
  "account",
  "models",
  "config",
  "mcp_servers",
  "plugins",
  "skills",
  "hooks",
  "apps",
];
type MutationKind = keyof typeof CODEX_MUTATIONS | "mcpConfig";

export function CodexControlView({
  client,
  onClose,
}: {
  client: Client | null;
  onClose: () => void;
}) {
  const { snapshot, events, operations, error, loading, refresh, dismissEvent } = useCodexControl(
    client,
    true,
  );
  const [resource, setResource] = useState<CodexResource>("account");
  const [kind, setKind] = useState<MutationKind>("pluginInstall");
  const [target, setTarget] = useState("");
  const [value, setValue] = useState("");
  const [source, setSource] = useState("");
  const [sha256, setSha256] = useState("");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("[]");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpEnv, setMcpEnv] = useState("");
  const [mcpRemove, setMcpRemove] = useState(false);
  const [operation, setOperation] = useState<CodexOperation | null>(null);
  const [capability, setCapability] = useState("");
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mutationState = useMemo<{ mutation: CodexMutation | null; error: string | null }>(() => {
    try {
      const params =
        kind === "mcpConfig"
          ? buildMcpConfigMutation({
              name: target,
              command: mcpCommand,
              args: mcpArgs,
              url: mcpUrl,
              env: mcpEnv,
              remove: mcpRemove,
            })
          : mutationParams(kind, target.trim(), value.trim(), source.trim(), sha256.trim());
      return {
        mutation: params
          ? { ...(kind === "mcpConfig" ? CODEX_MUTATIONS.config : CODEX_MUTATIONS[kind]), params }
          : null,
        error: params ? null : mutationValidationMessage(kind),
      };
    } catch (cause) {
      return { mutation: null, error: cause instanceof Error ? cause.message : String(cause) };
    }
  }, [kind, target, value, source, sha256, mcpCommand, mcpArgs, mcpUrl, mcpEnv, mcpRemove]);
  const mutation = mutationState.mutation;
  const inputRevision = JSON.stringify([
    kind,
    target,
    value,
    source,
    sha256,
    mcpCommand,
    mcpArgs,
    mcpUrl,
    mcpEnv,
    mcpRemove,
  ]);
  useEffect(() => {
    if (!inputRevision) return;
    setOperation(null);
    setCapability("");
  }, [inputRevision]);
  const pendingRequests = events.filter((event) => event.event.kind === "server_request");

  async function prepare() {
    if (!client || !mutation) return;
    await run(async () => {
      setOperation(await prepareCodexOperation(client, mutation));
      setCapability("");
    });
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
    setBusy(true);
    setMutationError(null);
    try {
      await action();
    } catch (cause) {
      setMutationError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="codex-control" aria-label="Codex app-server control">
      <header className="codex-control-header">
        <div>
          <span className="settings-eyebrow">Axon server</span>
          <h2>
            <Bot size={20} /> Codex app-server
          </h2>
          <p>{snapshot?.status.home ?? "Dedicated isolated control home"}</p>
        </div>
        <div className="codex-control-actions">
          <Button variant="plain" onClick={() => void refresh()} disabled={loading}>
            <RefreshCw size={15} /> Refresh
          </Button>
          <Button variant="plain" onClick={onClose} aria-label="Close Codex control">
            <X size={16} />
          </Button>
        </div>
      </header>
      {(error || mutationError) && (
        <p className="settings-error" role="alert">
          {error ?? mutationError}
        </p>
      )}
      <section className="codex-status-card">
        <strong>{snapshot?.status.state ?? (loading ? "starting" : "unavailable")}</strong>
        <span>
          {snapshot?.status.detail ?? "Mutations require policy and a single-use human approval."}
        </span>
      </section>
      {pendingRequests.length > 0 && (
        <section className="codex-mutation">
          <h3>Server approval requests</h3>
          {pendingRequests.map((event) => (
            <article key={`${event.cursor.boot_id}:${event.event.request_id}`}>
              <strong>{event.event.method}</strong>
              <pre>{JSON.stringify(event.event.params, null, 2)}</pre>
              <div className="codex-control-actions">
                <Button
                  disabled={busy || !client}
                  onClick={() =>
                    void run(async () => {
                      if (client) await respondToCodexServerRequest(client, event, false);
                      dismissEvent(event);
                    })
                  }
                >
                  Deny
                </Button>
                <Button
                  disabled={busy || !client}
                  onClick={() =>
                    void run(async () => {
                      if (client) await respondToCodexServerRequest(client, event, true);
                      dismissEvent(event);
                    })
                  }
                >
                  Approve
                </Button>
              </div>
            </article>
          ))}
        </section>
      )}
      {operations.length > 0 && (
        <section className="codex-mutation">
          <h3>Unfinished operations</h3>
          {operations.map((item) => (
            <article key={item.id}>
              <strong>
                #{item.id} {item.phase}
              </strong>
              <code>{item.request_digest}</code>
              {["ambiguous", "recovery_required", "rollback_required"].includes(item.phase) && (
                <Button
                  disabled={busy || !client}
                  onClick={() =>
                    void run(async () => {
                      if (client) await reconcileCodexOperation(client, item.id);
                      await refresh();
                    })
                  }
                >
                  Mark reconciled
                </Button>
              )}
            </article>
          ))}
        </section>
      )}
      <nav className="codex-tabs" aria-label="Codex resources">
        {resources.map((item) => (
          <Button
            key={item}
            variant="plain"
            className={resource === item ? "codex-tab-active" : ""}
            onClick={() => setResource(item)}
          >
            {item.replace("_", " ")}
          </Button>
        ))}
      </nav>
      <section className="codex-resource">
        <h3>{resource.replace("_", " ")}</h3>
        <pre>{JSON.stringify(snapshot?.[resource] ?? null, null, 2)}</pre>
      </section>
      <section className="codex-mutation">
        <h3>Approved change</h3>
        <p>
          Prepare the exact request, approve its digest, then execute the single-use capability.
        </p>
        <label>
          Workflow
          <select value={kind} onChange={(event) => setKind(event.target.value as MutationKind)}>
            <option value="accountLogin">Start account login</option>
            <option value="accountLoginCancel">Cancel account login</option>
            <option value="accountLogout">Log out account</option>
            <option value="config">Write config value</option>
            <option value="mcpConfig">Add, edit, or remove MCP definition</option>
            <option value="configBatch">Write config batch</option>
            <option value="mcpReload">Reload MCP servers</option>
            <option value="mcpOauth">Start MCP OAuth</option>
            <option value="pluginInstall">Install plugin</option>
            <option value="pluginUninstall">Uninstall plugin</option>
            <option value="marketplaceAdd">Add marketplace</option>
            <option value="marketplaceRemove">Remove marketplace</option>
            <option value="marketplaceUpgrade">Upgrade marketplace</option>
            <option value="skillConfig">Enable, disable, or configure skill</option>
            <option value="skillImport">Import standalone skill or agent config</option>
          </select>
        </label>
        {kind !== "configBatch" && (
          <label>
            Target
            <input
              value={target}
              onChange={(event) => setTarget(event.target.value)}
              placeholder="Config key, MCP server, plugin, marketplace, or skill"
            />
          </label>
        )}
        {kind === "mcpConfig" ? (
          <>
            <label>
              Command
              <input
                value={mcpCommand}
                onChange={(event) => setMcpCommand(event.target.value)}
                placeholder="Executable only"
                disabled={mcpRemove}
              />
            </label>
            <label>
              Arguments (JSON array)
              <input
                value={mcpArgs}
                onChange={(event) => setMcpArgs(event.target.value)}
                disabled={mcpRemove}
              />
            </label>
            <label>
              HTTPS URL
              <input
                value={mcpUrl}
                onChange={(event) => setMcpUrl(event.target.value)}
                placeholder="https://…"
                disabled={mcpRemove}
              />
            </label>
            <label>
              Environment secret references
              <textarea
                value={mcpEnv}
                onChange={(event) => setMcpEnv(event.target.value)}
                placeholder="TOKEN=env:MY_TOKEN"
                disabled={mcpRemove}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={mcpRemove}
                onChange={(event) => setMcpRemove(event.target.checked)}
              />{" "}
              Remove this MCP definition
            </label>
          </>
        ) : kind === "configBatch" ? (
          <label>
            Batch writes (JSON array or object)
            <textarea
              value={value}
              onChange={(event) => setValue(event.target.value)}
              placeholder={
                '[{"keyPath":"model","value":"gpt-5"},{"keyPath":"approval_policy","value":"on-request"}]'
              }
            />
          </label>
        ) : (
          <label>
            {kind === "config" ? "Value (JSON)" : "Value"}
            <input
              value={value}
              onChange={(event) => setValue(event.target.value)}
              placeholder="Value, enabled state, or OAuth provider"
            />
          </label>
        )}
        {mutationState.error && (
          <p className="settings-error" role="alert">
            {mutationState.error}
          </p>
        )}
        {(kind === "pluginInstall" || kind === "marketplaceAdd" || kind === "skillImport") && (
          <>
            <label>
              Pinned HTTPS source
              <input
                value={source}
                onChange={(event) => setSource(event.target.value)}
                placeholder="https://…"
              />
            </label>
            <label>
              SHA-256 digest
              <input
                value={sha256}
                onChange={(event) => setSha256(event.target.value)}
                maxLength={64}
              />
            </label>
          </>
        )}
        <div className="codex-control-actions">
          <Button disabled={busy || !mutation || !client} onClick={() => void prepare()}>
            1 Prepare
          </Button>
          <Button disabled={busy || !operation} onClick={() => void approve()}>
            2 Approve
          </Button>
          <Button disabled={busy || !capability} onClick={() => void execute()}>
            3 Execute
          </Button>
        </div>
        {operation && <pre>{JSON.stringify(operation, null, 2)}</pre>}
      </section>
    </main>
  );
}

function mutationParams(
  kind: Exclude<MutationKind, "mcpConfig">,
  target: string,
  value: string,
  source: string,
  sha256: string,
): Record<string, unknown> | null {
  if (
    kind === "accountLogin" ||
    kind === "accountLoginCancel" ||
    kind === "accountLogout" ||
    kind === "mcpReload"
  )
    return {};
  if (kind === "configBatch") return buildConfigBatchMutation(value);
  if (!target) return null;
  if (kind === "pluginInstall" || kind === "marketplaceAdd" || kind === "skillImport") {
    if (!source.startsWith("https://") || !/^[a-fA-F0-9]{64}$/.test(sha256)) return null;
    return { target, source, sha256 };
  }
  if (kind === "config") return { keyPath: target, value: parseConfigValue(value) };
  if (kind === "mcpOauth") return { name: target, provider: value || undefined };
  if (kind === "marketplaceRemove" || kind === "marketplaceUpgrade") {
    return { marketplaceName: target };
  }
  if (kind === "skillConfig") {
    const requested = parseConfigValue(value);
    if (typeof requested === "boolean") return { name: target, enabled: requested };
    throw new Error("Skill value must be true or false");
  }
  return { target };
}

function mutationValidationMessage(kind: MutationKind): string | null {
  if (kind === "configBatch") return "Enter a JSON batch containing at least two config writes";
  if (kind === "mcpConfig") return "Enter a valid MCP name and exactly one transport";
  if (kind === "pluginInstall" || kind === "marketplaceAdd" || kind === "skillImport")
    return "Enter a target, pinned HTTPS source, and 64-character SHA-256 digest";
  if (
    kind === "accountLogin" ||
    kind === "accountLoginCancel" ||
    kind === "accountLogout" ||
    kind === "mcpReload"
  )
    return null;
  return "Enter a target and valid value";
}
