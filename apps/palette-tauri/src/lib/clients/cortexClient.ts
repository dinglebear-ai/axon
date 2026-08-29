import type { BackendProfile, ProductIdentity } from "../backendProfiles/model";
import { backendRequest } from "./backendTransport";

export interface CortexLog {
  id: number;
  timestamp: string;
  received_at?: string;
  hostname: string;
  severity: string;
  app_name?: string | null;
  message: string;
  correlation_id?: string | null;
  redacted?: boolean;
  parse_warnings?: string[];
}
export interface CortexLogPage {
  logs: CortexLog[];
  next_cursor?: string | null;
  truncated?: boolean;
  source_watermark?: string | null;
}
export interface CortexFleetHost {
  hostname: string;
  status: string;
  last_seen_at?: string | null;
  age_seconds?: number | null;
  pressure_flags?: string[];
  degraded_reasons?: string[];
}
export interface CortexFleetState {
  hosts: CortexFleetHost[];
  summary?: Record<string, number>;
}
export interface CortexGraphEntity {
  id: number;
  entity_type: string;
  canonical_key: string;
  display_label: string;
  trust_level: string;
}
export interface CortexGraphRelationship {
  id: number;
  relationship_type: string;
  reason_code: string;
  confidence: number;
  evidence_count: number;
  evidence_ids: number[];
  src_entity?: CortexGraphEntity;
  dst_entity?: CortexGraphEntity;
}
export interface CortexGraphResult {
  entity?: CortexGraphEntity;
  candidates?: Array<{ entity: CortexGraphEntity; match_reason: string }>;
  relationships?: CortexGraphRelationship[];
  truncated?: boolean;
  continuation?: string | null;
  projection_status?: string;
  source_watermark?: string;
  degraded_reason?: string | null;
}
export const CORTEX_GRAPH_ENTITY_TYPES = [
  "host",
  "container",
  "logical_service",
  "service_instance",
  "app",
  "source_ip",
  "ai_project",
  "ai_session",
  "error_signature",
  "compose_project",
  "config_artifact",
  "domain",
  "network",
  "reverse_proxy",
  "storage",
  "git_commit",
  "user",
  "device",
] as const;
export type CortexGraphEntityType = (typeof CORTEX_GRAPH_ENTITY_TYPES)[number];
export interface CortexGraphSelector {
  entityType: CortexGraphEntityType;
  key: string;
}
export interface CortexCorrelationResult {
  reference_time?: string;
  logs: CortexLog[];
  hosts?: Record<string, CortexLog[]>;
  evidence_ids?: number[];
  truncated?: boolean;
  next_cursor?: string | null;
}
export type CortexSessionEventKind =
  | "user"
  | "assistant"
  | "tool"
  | "hook"
  | "reasoning"
  | "status"
  | "error"
  | "unknown";
export interface CortexSessionEvent {
  position: number;
  timestamp: string;
  kind: CortexSessionEventKind;
  text: string;
  redacted: boolean;
  parse_warning?: string;
}
export interface CortexRenderedSessionPage {
  contract_version: "1.0.0";
  delivery: "polling";
  events: CortexSessionEvent[];
  next_cursor: string;
  high_watermark: number;
  has_more: boolean;
  truncated_by_bytes: boolean;
  poll_after_ms: number;
  max_page_items: number;
  max_page_bytes: number;
}
export interface CortexSessionIdentity {
  project: string;
  tool: string;
  sessionId: string;
  host: string;
}
export interface CortexSessionSearchEntry {
  session_key: string;
  project: string;
  tool: string;
  session_id: string;
  hostname: string;
  first_seen: string;
  last_seen: string;
  event_count: number;
  match_count: number;
  best_snippet?: string | null;
}
export interface CortexSessionSearchResult {
  sessions: CortexSessionSearchEntry[];
  truncated: boolean;
  candidate_window_truncated: boolean;
}

function query(path: string, params: Record<string, string | number | undefined>) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params))
    if (value !== undefined && value !== "") search.set(key, String(value));
  const suffix = search.toString();
  return `${path}${suffix ? `?${suffix}` : ""}` as `/api/${string}`;
}
export class CortexClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "cortex") throw new Error("CortexClient requires a Cortex profile");
  }
  request<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: `/api/${string}` | "/v1/integration/identity",
    body?: unknown,
    signal?: AbortSignal,
  ) {
    return backendRequest<T>(this.profile, method, path, body, signal);
  }
  identity(signal?: AbortSignal) {
    return this.request<ProductIdentity>("GET", "/v1/integration/identity", undefined, signal);
  }

  logs(
    params: { query?: string; host?: string; severity?: string; cursor?: string; limit?: number },
    signal?: AbortSignal,
  ) {
    return this.request<CortexLogPage>("GET", query("/api/search", params), undefined, signal);
  }
  fleet(signal?: AbortSignal) {
    return this.request<CortexFleetState>("GET", "/api/fleet-state", undefined, signal);
  }
  graph(selector: CortexGraphSelector, signal?: AbortSignal) {
    return this.request<CortexGraphResult>(
      "GET",
      query("/api/graph/around", {
        entity_type: selector.entityType,
        key: selector.key,
        depth: 1,
        limit: 100,
        evidence_sample_limit: 3,
        payload_budget: 32768,
      }),
      undefined,
      signal,
    );
  }
  correlate(
    params: { query?: string; reference_time?: string; cursor?: string; limit?: number },
    signal?: AbortSignal,
  ) {
    return this.request<CortexCorrelationResult>(
      "GET",
      query("/api/correlate", params),
      undefined,
      signal,
    );
  }
  renderedSession(identity: CortexSessionIdentity, cursor?: string, signal?: AbortSignal) {
    return this.request<CortexRenderedSessionPage>(
      "GET",
      query("/api/sessions/rendered", {
        project: identity.project,
        tool: identity.tool,
        session_id: identity.sessionId,
        host: identity.host,
        cursor,
        limit: 200,
      }),
      undefined,
      signal,
    );
  }
  searchSessions(search: string, signal?: AbortSignal) {
    return this.request<CortexSessionSearchResult>(
      "GET",
      query("/api/sessions/search", { query: search, limit: 50 }),
      undefined,
      signal,
    );
  }
}
