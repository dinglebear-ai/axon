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
export interface CortexCorrelationResult {
  reference_time?: string;
  logs: CortexLog[];
  hosts?: Record<string, CortexLog[]>;
  evidence_ids?: number[];
  truncated?: boolean;
  next_cursor?: string | null;
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
    params: { q?: string; host?: string; severity?: string; cursor?: string; limit?: number },
    signal?: AbortSignal,
  ) {
    return this.request<CortexLogPage>("GET", query("/api/search", params), undefined, signal);
  }
  fleet(signal?: AbortSignal) {
    return this.request<CortexFleetState>("GET", "/api/fleet-state", undefined, signal);
  }
  graph(key: string, signal?: AbortSignal) {
    return this.request<CortexGraphResult>(
      "GET",
      query("/api/graph/around", {
        key,
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
}
