import { Bot, RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
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
  readCodexAction,
  respondToCodexServerRequest,
} from "@/lib/codexControl";
import { useCodexControl } from "@/lib/useCodexControl";
import { CodexMutationEditor } from "./CodexMutationEditor";
import { CodexOperationList } from "./CodexOperationList";

const resources: CodexResource[] = [
  "account",
  "models",
  "config",
  "mcp_servers",
  "plugins",
  "skills",
  "hooks",
  "apps",
  "method_inventory",
];
export type MutationKind = keyof typeof CODEX_MUTATIONS | "mcpConfig";
type ReadAction = Parameters<typeof readCodexAction>[1];
const advancedReads: ReadonlyArray<{ label: string; action: ReadAction }> = [
  { label: "Account rate limits", action: "rate_limits_read" },
  { label: "Account usage", action: "account_usage_read" },
  { label: "Workspace messages", action: "account_workspace_messages_read" },
  { label: "Amazon Bedrock discovery", action: "account_bedrock_discover" },
  {
    label: "Model provider capabilities",
    action: "model_provider_capabilities_read",
  },
  { label: "MCP resource", action: "mcp_server_resource_read" },
  { label: "Installed plugins", action: "plugins_installed" },
  { label: "Search plugins", action: "plugin_search" },
  { label: "Plugin detail", action: "plugin_read" },
  { label: "Plugin skill", action: "plugin_skill_read" },
  { label: "Plugin shares", action: "plugin_share_list" },
  {
    label: "External config detection",
    action: "external_agent_config_detect",
  },
  {
    label: "External import histories",
    action: "external_agent_config_import_read_histories",
  },
  { label: "Installed apps", action: "apps_installed" },
  { label: "App detail", action: "app_read" },
  { label: "Config requirements", action: "config_requirements_read" },
  { label: "Collaboration modes", action: "collaboration_modes_list" },
  { label: "Permission profiles", action: "permission_profiles_list" },
  { label: "Experimental features", action: "experimental_features_list" },
];

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
  const [readAction, setReadAction] = useState<ReadAction>(advancedReads[0].action);
  const [readParams, setReadParams] = useState("{}");
  const [readResult, setReadResult] = useState<unknown>(null);
  const [kind, setKind] = useState<MutationKind>("pluginInstall");
  const [target, setTarget] = useState("");
  const [value, setValue] = useState("");
  const [source, setSource] = useState("");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("[]");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpEnv, setMcpEnv] = useState("");
  const [mcpRemove, setMcpRemove] = useState(false);
  const [operation, setOperation] = useState<CodexOperation | null>(null);
  const [capability, setCapability] = useState("");
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [respondingRequest, setRespondingRequest] = useState<number | null>(null);
  const [serverResponses, setServerResponses] = useState<Record<number, string>>({});
  const mutationState = useMemo<{
    mutation: CodexMutation | null;
    error: string | null;
  }>(() => {
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
          : mutationParams(kind, target.trim(), value.trim(), source.trim());
      return {
        mutation: params
          ? {
              ...(kind === "mcpConfig" ? CODEX_MUTATIONS.config : CODEX_MUTATIONS[kind]),
              params,
            }
          : null,
        error: params ? null : mutationValidationMessage(kind),
      };
    } catch (cause) {
      return {
        mutation: null,
        error: cause instanceof Error ? cause.message : String(cause),
      };
    }
  }, [kind, target, value, source, mcpCommand, mcpArgs, mcpUrl, mcpEnv, mcpRemove]);
  const mutation = mutationState.mutation;
  const inputRevision = JSON.stringify([
    kind,
    target,
    value,
    source,
    mcpCommand,
    mcpArgs,
    mcpUrl,
    mcpEnv,
    mcpRemove,
  ]);
  const revisionRef = useRef(inputRevision);
  revisionRef.current = inputRevision;
  useEffect(() => {
    if (!inputRevision) return;
    setOperation(null);
    setCapability("");
  }, [inputRevision]);
  const pendingRequests = events.filter((event) => event.event.kind === "server_request");

  async function prepare() {
    if (!client || !mutation) return;
    const preparedRevision = inputRevision;
    await run(async () => {
      const prepared = await prepareCodexOperation(client, mutation);
      if (revisionRef.current !== preparedRevision) return;
      setOperation(prepared);
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
              {requiresTypedResponse(event.event.method) && event.event.request_id != null && (
                <label>
                  Typed response (JSON)
                  <textarea
                    value={serverResponses[event.event.request_id] ?? ""}
                    onChange={(change) => {
                      const requestId = event.event.request_id;
                      if (requestId == null) return;
                      setServerResponses((previous) => ({
                        ...previous,
                        [requestId]: change.target.value,
                      }));
                    }}
                    placeholder={typedResponsePlaceholder(event.event.method)}
                  />
                </label>
              )}
              <div className="codex-control-actions">
                <Button
                  disabled={respondingRequest === event.event.request_id || !client}
                  onClick={() => void respond(event, false)}
                >
                  Deny
                </Button>
                <Button
                  disabled={respondingRequest === event.event.request_id || !client}
                  onClick={() => void respond(event, true)}
                >
                  Approve
                </Button>
              </div>
            </article>
          ))}
        </section>
      )}
      <CodexOperationList
        operations={operations}
        client={client}
        busy={busy}
        run={run}
        refresh={refresh}
      />
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
        <h3>Detailed resources</h3>
        <label>
          Read action
          <select
            value={readAction}
            onChange={(event) => setReadAction(event.target.value as ReadAction)}
          >
            {advancedReads.map((item) => (
              <option key={item.action} value={item.action}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Method parameters (JSON)
          <textarea value={readParams} onChange={(event) => setReadParams(event.target.value)} />
        </label>
        <Button
          disabled={busy || !client}
          onClick={() =>
            void run(async () => {
              const parsed = parseConfigValue(readParams);
              if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
                throw new Error("Read parameters must be a JSON object");
              if (client)
                setReadResult(
                  await readCodexAction(client, readAction, parsed as Record<string, unknown>),
                );
            })
          }
        >
          Read resource
        </Button>
        {readResult !== null && <pre>{JSON.stringify(readResult, null, 2)}</pre>}
      </section>
      <CodexMutationEditor
        kind={kind}
        setKind={setKind}
        target={target}
        setTarget={setTarget}
        value={value}
        setValue={setValue}
        source={source}
        setSource={setSource}
        mcpCommand={mcpCommand}
        setMcpCommand={setMcpCommand}
        mcpArgs={mcpArgs}
        setMcpArgs={setMcpArgs}
        mcpUrl={mcpUrl}
        setMcpUrl={setMcpUrl}
        mcpEnv={mcpEnv}
        setMcpEnv={setMcpEnv}
        mcpRemove={mcpRemove}
        setMcpRemove={setMcpRemove}
        validationError={mutationState.error}
        busy={busy}
        canPrepare={Boolean(mutation && client)}
        operation={operation}
        capability={capability}
        onPrepare={prepare}
        onApprove={approve}
        onExecute={execute}
      />
    </main>
  );

  async function respond(event: import("@/lib/codexControl").CodexEvent, approved: boolean) {
    if (!client || event.event.request_id == null) return;
    setRespondingRequest(event.event.request_id);
    setMutationError(null);
    try {
      let response: Record<string, unknown> | undefined;
      if (approved && requiresTypedResponse(event.event.method)) {
        const raw = serverResponses[event.event.request_id] ?? "";
        const parsed: unknown = JSON.parse(raw);
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
          throw new Error("Typed server response must be a JSON object");
        }
        response = parsed as Record<string, unknown>;
      }
      await respondToCodexServerRequest(client, event, approved, response);
      dismissEvent(event);
    } catch (cause) {
      setMutationError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRespondingRequest(null);
    }
  }
}

function requiresTypedResponse(method?: string): boolean {
  return method === "item/tool/requestUserInput" || method === "mcpServer/elicitation/request";
}

function typedResponsePlaceholder(method?: string): string {
  return method === "item/tool/requestUserInput"
    ? '{"answers":{"question_id":["answer"]}}'
    : '{"action":"accept","content":{"field":"value"}}';
}

export function mutationParams(
  kind: Exclude<MutationKind, "mcpConfig">,
  target: string,
  value: string,
  source: string,
): Record<string, unknown> | null {
  if (
    kind === "accountLogin" ||
    kind === "accountLoginCancel" ||
    kind === "accountLogout" ||
    kind === "mcpReload"
  )
    return {};
  if (kind === "configBatch") return buildConfigBatchMutation(value);
  if (
    kind === "mcpTool" ||
    kind === "accountResetCredit" ||
    kind === "accountBedrockSetup" ||
    kind === "experimentalFeaturesSet" ||
    kind === "mcpStreamStart" ||
    kind === "mcpStreamStop" ||
    kind === "pluginShareCheckout" ||
    kind === "pluginShareSave" ||
    kind === "pluginShareDelete" ||
    kind === "pluginShareTargets" ||
    kind === "skillRoots" ||
    kind === "importHistory"
  ) {
    const parsed = parseConfigValue(value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
      throw new Error("This workflow requires a JSON object value");
    return parsed as Record<string, unknown>;
  }
  if (kind === "marketplaceAdd") {
    if (!source.startsWith("https://")) return null;
    return { source };
  }
  if (kind === "skillImport") {
    const migrationItems = parseConfigValue(value);
    if (!Array.isArray(migrationItems)) throw new Error("Migration items must be a JSON array");
    return { migrationItems, ...(target ? { source: target } : {}) };
  }
  if (!target) return null;
  if (kind === "pluginInstall") return { pluginName: target };
  if (kind === "pluginUninstall") return { pluginId: target };
  if (kind === "config")
    return {
      keyPath: target,
      value: parseConfigValue(value),
      mergeStrategy: "upsert",
    };
  if (kind === "mcpOauth") {
    if (!value) return { name: target };
    const options = parseConfigValue(value);
    if (!options || typeof options !== "object" || Array.isArray(options)) {
      throw new Error("MCP OAuth options must be a JSON object");
    }
    const supported = new Set(["clientRegistration", "scopes", "threadId", "timeoutSecs"]);
    const unsupported = Object.keys(options).find((key) => !supported.has(key));
    if (unsupported) throw new Error(`Unsupported MCP OAuth option: ${unsupported}`);
    return { ...(options as Record<string, unknown>), name: target };
  }
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
  if (
    [
      "mcpTool",
      "accountResetCredit",
      "accountBedrockSetup",
      "experimentalFeaturesSet",
      "mcpStreamStart",
      "mcpStreamStop",
      "pluginShareCheckout",
      "pluginShareSave",
      "pluginShareDelete",
      "pluginShareTargets",
      "skillRoots",
      "importHistory",
    ].includes(kind)
  )
    return "Enter the method-specific JSON object from the Codex capability schema";
  if (kind === "mcpConfig") return "Enter a valid MCP name and exactly one transport";
  if (kind === "marketplaceAdd") return "Enter an HTTPS marketplace source";
  if (kind === "skillImport") return "Enter migration items as a JSON array";
  if (
    kind === "accountLogin" ||
    kind === "accountLoginCancel" ||
    kind === "accountLogout" ||
    kind === "mcpReload"
  )
    return null;
  return "Enter a target and valid value";
}
