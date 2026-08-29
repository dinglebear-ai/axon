export const MAX_GATEWAY_ROWS = 100;
export const MAX_GATEWAY_ARGS = 20;
export const MAX_GATEWAY_PATTERNS = 50;

export interface GatewayConfig {
  name: string;
  enabled: boolean;
  url?: string | null;
  command?: string | null;
  args: string[];
  bearer_token_env?: string | null;
  oauth_enabled: boolean;
  proxy_resources: boolean;
  proxy_prompts: boolean;
  expose_tools?: string[] | null;
}
export interface GatewayRuntime {
  name: string;
  connected: boolean;
  tool_count: number;
  resource_count: number;
  prompt_count: number;
  last_error?: string | null;
}
export interface GatewayView {
  revision: string;
  config: GatewayConfig;
  runtime: GatewayRuntime;
}
export interface GatewayServerRow {
  id: string;
  source: string;
}
export interface GatewayDraft {
  name: string;
  enabled: boolean;
  transport: "http" | "stdio";
  url: string;
  command: string;
  args: string;
  bearerTokenEnv: string;
  oauthEnabled: boolean;
  proxyResources: boolean;
  proxyPrompts: boolean;
  exposeTools: string;
}
export interface GatewayChallenge {
  privateTarget: boolean;
  stdio: boolean;
  oauth: boolean;
  messages: string[];
}

export function emptyGatewayDraft(): GatewayDraft {
  return {
    name: "",
    enabled: true,
    transport: "http",
    url: "",
    command: "",
    args: "",
    bearerTokenEnv: "",
    oauthEnabled: false,
    proxyResources: false,
    proxyPrompts: false,
    exposeTools: "",
  };
}

export function draftFromGateway(view: GatewayView): GatewayDraft {
  return {
    name: view.config.name,
    enabled: view.config.enabled,
    transport: view.config.command ? "stdio" : "http",
    url: view.config.url ?? "",
    command: view.config.command ?? "",
    args: view.config.args.slice(0, MAX_GATEWAY_ARGS).join("\n"),
    bearerTokenEnv: view.config.bearer_token_env ?? "",
    oauthEnabled: view.config.oauth_enabled,
    proxyResources: view.config.proxy_resources,
    proxyPrompts: view.config.proxy_prompts,
    exposeTools: (view.config.expose_tools ?? []).slice(0, MAX_GATEWAY_PATTERNS).join("\n"),
  };
}

export function gatewayFingerprint(view: GatewayView): string {
  return view.revision;
}

export function gatewayChallenge(draft: GatewayDraft): GatewayChallenge {
  const privateTarget = draft.transport === "http" && isPrivateTarget(draft.url);
  const stdio = draft.transport === "stdio";
  const oauth = draft.oauthEnabled;
  const messages: string[] = [];
  if (privateTarget)
    messages.push(
      "This endpoint targets a private or local network address. Labby will enforce its SSRF policy and may reject redirects or the target.",
    );
  if (stdio)
    messages.push(
      "Testing or enabling this entry may start the configured command on the Labby host. Palette never starts it locally.",
    );
  if (oauth)
    messages.push(
      "OAuth authorization is completed and stored by Labby. Palette never receives the upstream OAuth tokens.",
    );
  return { privateTarget, stdio, oauth, messages };
}

export function isPrivateTarget(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    if (["localhost", "::1", "0.0.0.0"].includes(host)) return true;
    if (/^127\./.test(host) || /^10\./.test(host) || /^192\.168\./.test(host)) return true;
    const match = host.match(/^172\.(\d+)\./);
    return Boolean(match && Number(match[1]) >= 16 && Number(match[1]) <= 31);
  } catch {
    return false;
  }
}

export function boundedGatewayRows(value: unknown): GatewayServerRow[] {
  if (!Array.isArray(value)) throw new Error("Labby returned an invalid gateway list.");
  return (value as GatewayServerRow[])
    .filter((row) => row.source === "custom_gateway" && typeof row.id === "string")
    .slice(0, MAX_GATEWAY_ROWS);
}
