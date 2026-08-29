import { executeAxonRequest, type Client, type PaletteResult } from "./axonClient";

export type CodexResource = "account" | "models" | "config" | "mcp_servers" | "plugins" | "skills" | "hooks" | "apps";
export type CodexSnapshot = Record<CodexResource, unknown> & {
  status: { state: string; detail?: string | null; home?: string | null; binary?: string | null };
};
export type CodexOperation = { id: number; phase: string; request_digest: string };
export type CodexEvent = { cursor: { boot_id: number; sequence: number }; event: { kind: string; request_id?: number; method?: string; params?: unknown } };
export type CodexMutation = {
  action: string;
  method: string;
  scope: string;
  params: Record<string, unknown>;
  expectedRevision?: string | null;
};

export const CODEX_MUTATIONS = {
  accountLogin: { action: "account_login_start", method: "account/login/start", scope: "codex:account:write" },
  accountLoginCancel: { action: "account_login_cancel", method: "account/login/cancel", scope: "codex:account:write" },
  accountLogout: { action: "account_logout", method: "account/logout", scope: "codex:account:write" },
  config: { action: "config_value_write", method: "config/value/write", scope: "codex:config:write" },
  configBatch: { action: "config_batch_write", method: "config/batchWrite", scope: "codex:config:write" },
  mcpReload: { action: "mcp_server_reload", method: "config/mcpServer/reload", scope: "codex:mcp:write" },
  mcpOauth: { action: "mcp_server_oauth_login", method: "mcpServer/oauth/login", scope: "codex:mcp:write" },
  pluginInstall: { action: "plugin_install", method: "plugin/install", scope: "codex:plugins:write" },
  pluginUninstall: { action: "plugin_uninstall", method: "plugin/uninstall", scope: "codex:plugins:write" },
  marketplaceAdd: { action: "marketplace_add", method: "marketplace/add", scope: "codex:plugins:write" },
  marketplaceRemove: { action: "marketplace_remove", method: "marketplace/remove", scope: "codex:plugins:write" },
  marketplaceUpgrade: { action: "marketplace_upgrade", method: "marketplace/upgrade", scope: "codex:plugins:write" },
  skillConfig: { action: "skill_config_write", method: "skills/config/write", scope: "codex:skills:write" },
  skillImport: { action: "external_agent_config_import", method: "externalAgentConfig/import", scope: "codex:skills:write" },
} as const;

export async function readCodexSnapshot(client: Client): Promise<CodexSnapshot> {
  return payload<CodexSnapshot>(await executeAxonRequest(client, "GET", "/v1/codex"));
}

export async function readCodexOperations(client: Client): Promise<CodexOperation[]> {
  return payload<CodexOperation[]>(await executeAxonRequest(client, "GET", "/v1/codex/operations"));
}

export async function reconcileCodexOperation(client: Client, id: number, revision: string): Promise<void> {
  await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/reconcile`, { revision });
}

export async function readCodexEvents(client: Client, cursor?: { boot_id: number; sequence: number }): Promise<CodexEvent[]> {
  const query = cursor ? `?limit=100&boot_id=${cursor.boot_id}&after=${cursor.sequence}` : "?limit=100";
  return payload<CodexEvent[]>(await executeAxonRequest(client, "GET", `/v1/codex/events${query}`));
}

export async function respondToCodexServerRequest(client: Client, event: CodexEvent, approved: boolean): Promise<void> {
  const requestId = event.event.request_id;
  if (requestId == null) throw new Error("Codex event has no server request id");
  await executeAxonRequest(client, "POST", `/v1/codex/server-requests/${requestId}/respond`, {
    boot_id: event.cursor.boot_id,
    approved,
  });
}

export async function prepareCodexOperation(client: Client, mutation: CodexMutation): Promise<CodexOperation> {
  return payload<CodexOperation>(await executeAxonRequest(client, "POST", "/v1/codex/operations", {
    method: mutation.method,
    expected_revision: mutation.expectedRevision ?? null,
    idempotency_key: crypto.randomUUID(),
    redacted_request: mutation.params,
  }));
}

export async function approveCodexOperation(client: Client, id: number): Promise<string> {
  const result = await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/approve`, {});
  return payload<{ approval_capability: string }>(result).approval_capability;
}

export async function executeCodexOperation(client: Client, id: number, capability: string, mutation: CodexMutation): Promise<unknown> {
  return payload(await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/execute`, {
    capability,
    action: mutation.action,
    params: mutation.params,
    revision: mutation.expectedRevision ?? null,
  }));
}

function payload<T = unknown>(result: PaletteResult): T {
  if (!result.ok) {
    const message = typeof result.payload === "object" && result.payload && "error" in result.payload
      ? String((result.payload as { error: unknown }).error)
      : `Axon request failed (${result.status})`;
    throw new Error(message);
  }
  return result.payload as T;
}
