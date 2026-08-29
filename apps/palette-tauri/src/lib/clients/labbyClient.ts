import type { BackendProfile } from "../backendProfiles/model";
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
