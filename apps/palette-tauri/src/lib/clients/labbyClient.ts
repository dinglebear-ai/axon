import type { BackendProfile, ProductIdentity } from "../backendProfiles/model";
import { backendRequest } from "./backendTransport";

export type JsonSchema = Record<string, unknown>;
export interface LabbyCatalogEntry {
  kind: "mcpTool" | "labbyAction";
  id: string;
  label: string;
  description: string;
  source: string;
  destructive: boolean;
  contractHash: string;
  upstream?: string;
  tool?: string;
}
export interface LabbyCatalog {
  fingerprint: string;
  entries: LabbyCatalogEntry[];
}
export interface LabbyToolDescriptor extends LabbyCatalogEntry {
  contractVersion: number;
  catalogRevision: string;
  inputSchema: JsonSchema | null;
  outputSchema: JsonSchema | null;
  annotations: Record<string, boolean | null>;
}
export interface LabbyExactReceipt {
  requestId: string;
  auditId: string;
  toolId: string;
  contractHash: string;
  catalogRevision: string;
  executionMode: "exact";
  llmInvocations: 0;
  truncated: boolean;
}
export interface LabbyExactResult {
  id: string;
  result: unknown;
  receipt: LabbyExactReceipt;
  ui?: unknown;
}
export interface LabbySnippetInfo {
  name: string;
  description?: string | null;
  source: "builtin" | "user";
  path?: string;
  shadowed?: boolean;
}
export interface LabbyResolvedSnippet extends LabbySnippetInfo {
  body: string;
}
export interface LabbySnippetReceipt<T = unknown> {
  value: T;
  receipt: LabbyExactReceipt;
}
export type CapabilityFamily =
  | "tool"
  | "prompt"
  | "resource"
  | "skill"
  | "agent"
  | "mcp_app"
  | "mcp_server"
  | "plugin";
export interface CapabilityRef {
  provider: string;
  family: CapabilityFamily;
  memberId: string;
  expectedRevision: string;
}
export interface ExecutionLoadoutSummary {
  id: string;
  name: string;
  draftRevision: number;
  desiredActiveRevision: number | null;
  effectiveRuntimeRevision: number | null;
}
export interface ExecutionLoadoutDraft extends ExecutionLoadoutSummary {
  description: string | null;
  members: CapabilityRef[];
  restartRequired: boolean;
}
export type ResolutionStatus = "effective" | "missing" | "stale" | "unauthorized" | "unsupported";
export interface ResolvedCapability {
  capability: CapabilityRef;
  status: ResolutionStatus;
  currentRevision: string | null;
  diagnostic: string | null;
}
export interface ExecutionLoadoutPreview {
  loadoutId: string;
  draftRevision: number;
  catalogGeneration: string;
  principal: string;
  runtimeIdentity: string;
  resolved: ResolvedCapability[];
  effective: CapabilityRef[];
  missing: CapabilityRef[];
  conflicts: string[];
}
export interface ExecutionLoadoutActivation {
  loadout: ExecutionLoadoutDraft;
  revision: {
    loadoutId: string;
    revision: number;
    members: CapabilityRef[];
    catalogGeneration: string;
  };
  preview: ExecutionLoadoutPreview;
}
export interface ExecutionLoadoutConflict {
  kind: "stale_revision";
  expected: number;
  current: number;
  changedFields: string[];
}
export class LabbyClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "labby") throw new Error("LabbyClient requires a Labby profile");
  }
  private request<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: `/v1/palette/${string}`,
    body?: unknown,
    signal?: AbortSignal,
  ) {
    return backendRequest<T>(this.profile, method, path, body, signal);
  }

  identity(signal?: AbortSignal) {
    return backendRequest<ProductIdentity>(
      this.profile,
      "GET",
      "/v1/integration/identity",
      null,
      signal,
    );
  }

  private async loadoutRequest<T>(
    method: "GET" | "POST" | "PATCH",
    path: `/v1/palette/execution-loadouts${string}`,
    body?: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    const response = await this.request<T>(method, path, body, signal);
    if (!response.ok)
      throw Object.assign(new Error(`Labby loadout request failed (${response.status})`), {
        detail: response.payload,
      });
    return response.payload;
  }

  listExecutionLoadouts(signal?: AbortSignal) {
    return this.loadoutRequest<{ items: ExecutionLoadoutSummary[] }>(
      "GET",
      "/v1/palette/execution-loadouts",
      undefined,
      signal,
    );
  }
  getExecutionLoadout(id: string, signal?: AbortSignal) {
    return this.loadoutRequest<ExecutionLoadoutDraft>(
      "GET",
      `/v1/palette/execution-loadouts/${encodeURIComponent(id)}`,
      undefined,
      signal,
    );
  }
  createExecutionLoadout(
    input: { id: string; name: string; description?: string | null; members: CapabilityRef[] },
    signal?: AbortSignal,
  ) {
    return this.loadoutRequest<ExecutionLoadoutDraft>(
      "POST",
      "/v1/palette/execution-loadouts",
      input,
      signal,
    );
  }
  patchExecutionLoadout(
    id: string,
    input: {
      expectedDraftRevision: number;
      name?: string;
      description?: string | null;
      members?: CapabilityRef[];
    },
    signal?: AbortSignal,
  ) {
    return this.loadoutRequest<ExecutionLoadoutDraft>(
      "PATCH",
      `/v1/palette/execution-loadouts/${encodeURIComponent(id)}`,
      input,
      signal,
    );
  }
  previewExecutionLoadout(id: string, runtimeIdentity: string, signal?: AbortSignal) {
    return this.loadoutRequest<ExecutionLoadoutPreview>(
      "POST",
      `/v1/palette/execution-loadouts/${encodeURIComponent(id)}/preview`,
      { runtimeIdentity },
      signal,
    );
  }
  activateExecutionLoadout(
    id: string,
    expectedDraftRevision: number,
    runtimeIdentity: string,
    signal?: AbortSignal,
  ) {
    return this.loadoutRequest<ExecutionLoadoutActivation>(
      "POST",
      `/v1/palette/execution-loadouts/${encodeURIComponent(id)}/activate`,
      { expectedDraftRevision, runtimeIdentity },
      signal,
    );
  }
  rollbackExecutionLoadout(
    id: string,
    expectedDraftRevision: number,
    revision: number,
    signal?: AbortSignal,
  ) {
    return this.loadoutRequest<ExecutionLoadoutDraft>(
      "POST",
      `/v1/palette/execution-loadouts/${encodeURIComponent(id)}/rollback`,
      { expectedDraftRevision, revision, runtimeIdentity: "palette" },
      signal,
    );
  }

  async search(query: string, signal?: AbortSignal): Promise<LabbyCatalog> {
    const params = new URLSearchParams({ q: query, limit: "100" });
    return (
      await this.request<LabbyCatalog>("GET", `/v1/palette/search?${params}`, undefined, signal)
    ).payload;
  }

  async descriptor(id: string, signal?: AbortSignal): Promise<LabbyToolDescriptor> {
    const params = new URLSearchParams({ id });
    return (
      await this.request<LabbyToolDescriptor>(
        "GET",
        `/v1/palette/descriptor?${params}`,
        undefined,
        signal,
      )
    ).payload;
  }

  async execute(
    descriptor: LabbyToolDescriptor,
    params: Record<string, unknown>,
    confirmDestructive: boolean,
    signal?: AbortSignal,
  ): Promise<LabbyExactResult> {
    const response = await this.request<LabbyExactResult>(
      "POST",
      "/v1/palette/execute",
      {
        id: descriptor.id,
        params,
        expectedContractHash: descriptor.contractHash,
        confirmDestructive,
      },
      signal,
    );
    if (!response.ok) throw new Error(`Labby exact call failed (${response.status})`);
    if (
      response.payload.receipt.executionMode !== "exact" ||
      response.payload.receipt.llmInvocations !== 0
    ) {
      throw new Error("Labby did not confirm an exact no-LLM execution");
    }
    return response.payload;
  }

  private async snippetAction<T>(
    action: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<LabbySnippetReceipt<T>> {
    const id = `labby:snippets::${action}`;
    const catalog = await this.search(id, signal);
    const entry = catalog.entries.find((candidate) => candidate.id === id);
    if (!entry) throw new Error(`Labby does not expose ${action} for this principal`);
    const descriptor = await this.descriptor(entry.id, signal);
    const result = await this.execute(descriptor, params, descriptor.destructive, signal);
    return { value: result.result as T, receipt: result.receipt };
  }

  listSnippets(signal?: AbortSignal) {
    return this.snippetAction<{ snippets: LabbySnippetInfo[] }>("snippets.list", {}, signal);
  }
  getSnippet(name: string, signal?: AbortSignal) {
    return this.snippetAction<LabbyResolvedSnippet>("snippets.get", { name }, signal);
  }
  validateSnippet(name: string, body: string, signal?: AbortSignal) {
    return this.snippetAction<{ valid: true; name: string; mode: string }>(
      "snippets.validate",
      { name, body },
      signal,
    );
  }
  createSnippet(
    draft: { name: string; body: string; description: string; force: boolean },
    signal?: AbortSignal,
  ) {
    return this.snippetAction<LabbySnippetInfo>("snippets.create", draft, signal);
  }
  testSnippet(name: string, params: Record<string, unknown>, signal?: AbortSignal) {
    return this.snippetAction<{ name: string; passed: boolean; response: unknown }>(
      "snippets.test",
      { name, params },
      signal,
    );
  }
  executeSnippet(name: string, params: Record<string, unknown>, signal?: AbortSignal) {
    return this.snippetAction<unknown>("snippets.exec", { name, params }, signal);
  }
  promoteSnippet(
    draft: {
      execution_id: string;
      name: string;
      description: string;
      force: boolean;
      shadow_builtin: boolean;
    },
    signal?: AbortSignal,
  ) {
    return this.snippetAction<unknown>("snippets.promote", draft, signal);
  }
  removeSnippet(name: string, signal?: AbortSignal) {
    return this.snippetAction<unknown>("snippets.remove", { name }, signal);
  }
}
