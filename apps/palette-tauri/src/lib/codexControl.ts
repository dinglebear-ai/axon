import { type Client, executeAxonRequest, type PaletteResult } from "./axonClient";

export type CodexResource =
  | "account"
  | "models"
  | "config"
  | "mcp_servers"
  | "plugins"
  | "skills"
  | "hooks"
  | "apps";
export type CodexSnapshot = Record<CodexResource, unknown> & {
  status: { state: string; detail?: string | null; home?: string | null; binary?: string | null };
  pending_server_requests: CodexEvent[];
};
export type CodexOperation = { id: number; phase: string; request_digest: string };
export type CodexEvent = {
  cursor: { boot_id: number; sequence: number };
  event: { kind: string; request_id?: number; method?: string; params?: unknown };
};
export type CodexMutation = {
  action: string;
  params: Record<string, unknown>;
};
export type McpConfigInput = {
  name: string;
  command: string;
  args: string;
  url: string;
  env: string;
  remove: boolean;
};

type ConfigWrite = { keyPath: string | string[]; value: unknown };

export const CODEX_MUTATIONS = {
  accountLogin: {
    action: "account_login_start",
  },
  accountLoginCancel: {
    action: "account_login_cancel",
  },
  accountLogout: {
    action: "account_logout",
  },
  config: {
    action: "config_value_write",
  },
  configBatch: {
    action: "config_batch_write",
  },
  mcpReload: {
    action: "mcp_server_reload",
  },
  mcpOauth: {
    action: "mcp_server_oauth_login",
  },
  pluginInstall: {
    action: "plugin_install",
  },
  pluginUninstall: {
    action: "plugin_uninstall",
  },
  marketplaceAdd: {
    action: "marketplace_add",
  },
  marketplaceRemove: {
    action: "marketplace_remove",
  },
  marketplaceUpgrade: {
    action: "marketplace_upgrade",
  },
  skillConfig: {
    action: "skill_config_write",
  },
  skillImport: {
    action: "external_agent_config_import",
  },
} as const;

export async function readCodexSnapshot(client: Client): Promise<CodexSnapshot> {
  return payload<CodexSnapshot>(await executeAxonRequest(client, "GET", "/v1/codex"));
}

export async function readCodexOperations(client: Client): Promise<CodexOperation[]> {
  return payload<CodexOperation[]>(await executeAxonRequest(client, "GET", "/v1/codex/operations"));
}

export async function reconcileCodexOperation(client: Client, id: number): Promise<void> {
  await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/reconcile`, {});
}

export async function readCodexEvents(
  client: Client,
  cursor?: { boot_id: number; sequence: number },
): Promise<CodexEvent[]> {
  const query = cursor
    ? `?limit=100&boot_id=${cursor.boot_id}&after=${cursor.sequence}`
    : "?limit=100";
  return payload<CodexEvent[]>(await executeAxonRequest(client, "GET", `/v1/codex/events${query}`));
}

export async function respondToCodexServerRequest(
  client: Client,
  event: CodexEvent,
  approved: boolean,
): Promise<void> {
  const requestId = event.event.request_id;
  if (requestId == null) throw new Error("Codex event has no server request id");
  await executeAxonRequest(client, "POST", `/v1/codex/server-requests/${requestId}/respond`, {
    boot_id: event.cursor.boot_id,
    approved,
  });
}

export async function prepareCodexOperation(
  client: Client,
  mutation: CodexMutation,
): Promise<CodexOperation> {
  return payload<CodexOperation>(
    await executeAxonRequest(client, "POST", "/v1/codex/operations", {
      action: mutation.action,
      idempotency_key: crypto.randomUUID(),
      redacted_request: mutation.params,
    }),
  );
}

export async function approveCodexOperation(client: Client, id: number): Promise<string> {
  const result = await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/approve`, {});
  return payload<{ approval_capability: string }>(result).approval_capability;
}

export async function executeCodexOperation(
  client: Client,
  id: number,
  capability: string,
  mutation: CodexMutation,
): Promise<unknown> {
  return payload(
    await executeAxonRequest(client, "POST", `/v1/codex/operations/${id}/execute`, {
      capability,
      action: mutation.action,
      params: mutation.params,
    }),
  );
}

export function buildMcpConfigMutation(input: McpConfigInput): Record<string, unknown> {
  const name = input.name.trim();
  if (!/^[A-Za-z0-9_-]+$/.test(name))
    throw new Error("MCP name may contain only letters, numbers, underscores, and hyphens");
  if (input.remove) return { keyPath: `mcp_servers.${name}`, value: null };

  const command = input.command.trim();
  const url = input.url.trim();
  if ((!command && !url) || (command && url))
    throw new Error("Choose either command or URL for the MCP transport");
  if (command && /\s/.test(command))
    throw new Error("Command must be one executable; put parameters in args");
  if (url && !/^https:\/\//.test(url)) throw new Error("MCP URL must use HTTPS");

  let args: string[] = [];
  if (input.args.trim()) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(input.args);
    } catch {
      throw new Error("MCP args must be a JSON array of strings");
    }
    if (!Array.isArray(parsed) || !parsed.every((value) => typeof value === "string"))
      throw new Error("MCP args must be a JSON array of strings");
    args = parsed;
  }
  const env: Record<string, string> = {};
  for (const line of input.env
    .split("\n")
    .map((value) => value.trim())
    .filter(Boolean)) {
    const separator = line.indexOf("=");
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    if (
      separator < 1 ||
      !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ||
      !/^env:[A-Za-z_][A-Za-z0-9_]*$/.test(value)
    ) {
      throw new Error(
        "Each MCP environment value must be an env: secret reference (NAME=env:SECRET_NAME)",
      );
    }
    env[key] = value;
  }
  const value = command
    ? { command, ...(args.length ? { args } : {}), ...(Object.keys(env).length ? { env } : {}) }
    : (() => {
        if (args.length) throw new Error("URL MCP transports do not accept command arguments");
        if (Object.keys(env).length)
          throw new Error("URL MCP transports do not accept environment entries");
        return { url };
      })();
  return { keyPath: `mcp_servers.${name}`, value };
}

export function parseConfigValue(input: string): unknown {
  try {
    return JSON.parse(input);
  } catch {
    throw new Error('Config value must be valid JSON (for example: true, 42, "text", or {})');
  }
}

export function buildConfigBatchMutation(input: string): Record<string, unknown> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(input);
  } catch {
    throw new Error("Config batch must be valid JSON");
  }

  const container = Array.isArray(parsed) ? { edits: parsed } : parsed;
  if (!isRecord(container))
    throw new Error("Config batch must be an array of writes or an object containing edits");
  const field = ["edits", "writes", "changes"].find((key) => key in container);
  if (!field || !Array.isArray(container[field]))
    throw new Error("Config batch must contain an edits, writes, or changes array");
  if (container[field].length < 2)
    throw new Error(
      "Config batch must contain at least two writes; use Write config value for one",
    );
  for (const edit of container[field]) validateConfigWrite(edit);
  return container;
}

function validateConfigWrite(value: unknown): asserts value is ConfigWrite {
  if (!isRecord(value) || !("value" in value))
    throw new Error("Each config batch write must contain keyPath and value");
  const keyPath = value.keyPath ?? value.key_path ?? value.key;
  const validPath =
    (typeof keyPath === "string" && keyPath.trim().length > 0) ||
    (Array.isArray(keyPath) &&
      keyPath.length > 0 &&
      keyPath.every((part) => typeof part === "string" && part.trim().length > 0));
  if (!validPath) throw new Error("Each config batch write needs a non-empty keyPath");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function payload<T = unknown>(result: PaletteResult): T {
  if (!result.ok) {
    const message =
      typeof result.payload === "object" && result.payload && "error" in result.payload
        ? String((result.payload as { error: unknown }).error)
        : `Axon request failed (${result.status})`;
    throw new Error(message);
  }
  return result.payload as T;
}
