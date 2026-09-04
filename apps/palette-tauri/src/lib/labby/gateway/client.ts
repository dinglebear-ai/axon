import type { BackendProfile } from "@/lib/backendProfiles/model";
import { backendRequest } from "@/lib/clients/backendTransport";
import {
  boundedGatewayRows,
  type GatewayDraft,
  type GatewayRuntime,
  type GatewayView,
  gatewayFingerprint,
  MAX_GATEWAY_ARGS,
  MAX_GATEWAY_PATTERNS,
} from "./model";

export class GatewayClientError extends Error {
  constructor(
    message: string,
    readonly kind = "gateway_error",
    readonly status = 0,
  ) {
    super(message);
    this.name = "GatewayClientError";
  }
}

export class StaleGatewayError extends GatewayClientError {
  constructor() {
    super(
      "Gateway state changed on the server. Refresh and review the current values before retrying.",
      "stale_state",
      409,
    );
  }
}

function lines(value: string, max: number): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, max);
}

function specFromDraft(draft: GatewayDraft) {
  return {
    name: draft.name.trim(),
    enabled: draft.enabled,
    url: draft.transport === "http" ? draft.url.trim() : null,
    command: draft.transport === "stdio" ? draft.command.trim() : null,
    args: draft.transport === "stdio" ? lines(draft.args, MAX_GATEWAY_ARGS) : [],
    bearer_token_env: draft.bearerTokenEnv.trim() || null,
    oauth: draft.oauthEnabled
      ? {
          mode: "authorization_code_pkce",
          registration: { strategy: "dynamic" },
          scopes: null,
        }
      : null,
    proxy_resources: draft.proxyResources,
    proxy_prompts: draft.proxyPrompts,
    expose_tools: draft.exposeTools.trim() ? lines(draft.exposeTools, MAX_GATEWAY_PATTERNS) : null,
  };
}

export class GatewayClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "labby") throw new Error("GatewayClient requires a Labby profile");
  }

  private async action<T>(action: string, params: object, signal?: AbortSignal): Promise<T> {
    const response = await backendRequest<T>(
      this.profile,
      "POST",
      "/v1/gateway",
      { action, params },
      signal,
    );
    if (!response.ok) {
      const payload = response.payload as Record<string, unknown> | null;
      const kind = payload && typeof payload.kind === "string" ? payload.kind : "gateway_error";
      const message =
        payload && typeof payload.message === "string"
          ? payload.message
          : `Labby gateway request failed (${response.status})`;
      throw new GatewayClientError(message, kind, response.status);
    }
    return response.payload;
  }

  async list(signal?: AbortSignal): Promise<GatewayView[]> {
    const rows = boundedGatewayRows(await this.action<unknown>("gateway.list", {}, signal));
    const views: GatewayView[] = [];
    for (let offset = 0; offset < rows.length; offset += 6) {
      views.push(
        ...(await Promise.all(
          rows.slice(offset, offset + 6).map((row) => this.get(row.id, signal)),
        )),
      );
    }
    return views;
  }

  get(name: string, signal?: AbortSignal): Promise<GatewayView> {
    return this.action("gateway.get", { name }, signal);
  }
  testDraft(draft: GatewayDraft, signal?: AbortSignal): Promise<GatewayRuntime> {
    return this.action("gateway.test", { spec: specFromDraft(draft) }, signal);
  }
  create(draft: GatewayDraft, signal?: AbortSignal): Promise<GatewayView> {
    return this.action("gateway.add", { spec: specFromDraft(draft) }, signal);
  }
  async update(
    name: string,
    draft: GatewayDraft,
    expectedFingerprint: string,
    signal?: AbortSignal,
  ): Promise<GatewayView> {
    await this.assertCurrent(name, expectedFingerprint, signal);
    return this.action(
      "gateway.update",
      { name, expected_revision: expectedFingerprint, patch: specFromDraft(draft) },
      signal,
    );
  }
  async remove(name: string, expectedFingerprint: string, signal?: AbortSignal): Promise<void> {
    await this.assertCurrent(name, expectedFingerprint, signal);
    await this.action("gateway.remove", { name, expected_revision: expectedFingerprint }, signal);
  }
  async reload(
    name: string,
    expectedFingerprint: string,
    signal?: AbortSignal,
  ): Promise<GatewayView> {
    await this.assertCurrent(name, expectedFingerprint, signal);
    await this.action("gateway.reload", { name, expected_revision: expectedFingerprint }, signal);
    return this.get(name, signal);
  }

  private async assertCurrent(name: string, expected: string, signal?: AbortSignal): Promise<void> {
    if (gatewayFingerprint(await this.get(name, signal)) !== expected)
      throw new StaleGatewayError();
  }
}
