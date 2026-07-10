export type PanelState = {
  setup_required: boolean;
  config_path: string;
};

export type ConfigResponse = {
  path: string;
  raw_toml: string;
  restart_required: boolean;
};

export type EnvConfigResponse = {
  path: string;
  raw_env: string;
  restart_required: boolean;
};

export type SaveConfigResponse = {
  ok: boolean;
  restart_required: boolean;
  message: string;
};

export type StackCheck = {
  label: string;
  status: 'ok' | 'warn' | 'error' | 'skipped' | string;
  detail: string;
};

export type StackUrlCheck = StackCheck & {
  url: string;
};

export type StackResponse = {
  runtime_mode: 'host' | 'container' | string;
  server_url: string;
  mcp_url: string;
  log_dir: string;
  compose_file: string;
  urls: StackUrlCheck[];
  checks: StackCheck[];
};

export type PanelStatusResponse = {
  payload: {
    local_crawl_jobs?: ServiceJob[];
    local_extract_jobs?: ServiceJob[];
    local_embed_jobs?: ServiceJob[];
    local_ingest_jobs?: ServiceJob[];
    totals?: Record<string, number>;
  };
  text: string;
  totals: Record<string, number>;
};

export type ServiceJob = {
  id: string;
  status: string;
  updated_at: string;
  created_at: string;
  kind?: 'crawl' | 'extract' | 'embed' | 'ingest';
  error_text?: string | null;
  url?: string | null;
  target?: string | null;
  source_type?: string | null;
  urls_json?: unknown;
};

export type ArtifactHandle = {
  relative_path: string;
  bytes?: number;
  kind: string;
  display_path: string;
  line_count?: number;
};

export type PanelCommandResponse = {
  command: string;
  action: unknown;
  result: unknown;
};

export type CommandResultView = {
  ok: boolean;
  title: string;
  subtitle: string;
  rows: Array<{ label: string; value: string }>;
  body?: string;
  raw?: string;
  imageUrl?: string;
  imageArtifact?: ArtifactHandle;
  artifacts?: ArtifactHandle[];
};

export type PanelDoctorResponse = {
  payload: {
    observed_at_utc?: string;
    all_ok?: boolean;
    services?: Record<string, DoctorService>;
    pipelines?: Record<string, boolean>;
    browser_runtime?: {
      selection?: string;
    };
  };
};

export type DoctorService = {
  ok?: boolean;
  url?: string | null;
  detail?: string | null;
  model?: string | null;
  collection?: string | null;
  vector_mode?: string | null;
  path?: string | null;
  exists?: boolean;
  command?: string | null;
};

export type CheckSummary = {
  ok: number;
  warn: number;
  error: number;
  skipped: number;
  total: number;
};

export type ConfigFile = 'toml' | 'env';
export type PanelTab = 'dashboard' | 'jobs' | 'sources' | 'watches' | 'memory' | 'configurator';

export const TOKEN_KEY = 'axon-panel-token';

// ---------------------------------------------------------------------------
// Sources (GET/POST /v1/sources)
// ---------------------------------------------------------------------------
//
// The OpenAPI schema for `GET /v1/sources` is intentionally untyped
// (`schema: {}`) because the live handler
// (crates/axon-web/src/server/handlers/discovery.rs -> axon_services::system::sources)
// still returns the legacy "sources facet" shape (`count`/`limit`/`offset`/
// `urls: [{url, chunks}]`), not the ledger `Page<SourceSummary>` shape the
// pipeline-unification contract targets (`SourceService::list` is wired but
// returns `not_implemented` in production — see
// crates/axon-services/src/service_traits/source_service.rs). These types
// tolerate both shapes so the UI renders correctly today and keeps working
// if/when the server ships the ledger listing.
export type SourceListEntry = {
  url?: string;
  canonical_uri?: string;
  chunks?: number;
  source_kind?: string;
  status?: string;
  adapter?: { name?: string; version?: string } | string | null;
  counts?: {
    items_total?: number;
    documents_total?: number;
    chunks_total?: number;
    vector_points_total?: number;
  };
};

export type SourcesListResult = {
  count?: number;
  limit?: number;
  offset?: number;
  total?: number;
  next_cursor?: string | null;
  urls?: SourceListEntry[];
  items?: SourceListEntry[];
};

// ---------------------------------------------------------------------------
// Watches (GET/PATCH/DELETE /v1/watches, POST /v1/watches/{id}/{pause,resume})
// ---------------------------------------------------------------------------
//
// Mirrors crates/axon-api/src/source/job_listing.rs (WatchSummary,
// WatchUpdateRequest) and crates/axon-api/src/source/lifecycle.rs
// (WatchResult, WatchSchedule). Hand-typed rather than generated because the
// checked-in apps/web/openapi/axon.json snapshot predates the /v1/watches
// route landing (crates/axon-web/src/server/handlers/source_watch.rs) and a
// fresh export requires a running server build outside this territory. All
// *Id newtypes (WatchId/SourceId/JobId) serialize transparently as plain
// strings — see crates/axon-api/src/source/ids.rs.
export type WatchSchedule = {
  every_seconds: number;
  cron?: string | null;
  timezone?: string | null;
};

export type WatchSummary = {
  watch_id: string;
  source_id: string;
  enabled: boolean;
  schedule: WatchSchedule;
  next_run_at: string;
  last_job_id?: string | null;
  last_status?: string | null;
};

export type WatchPage = {
  items: WatchSummary[];
  next_cursor?: string | null;
  limit: number;
  total?: number;
};

export type WatchAdapterRef = {
  name: string;
  version: string;
};

export type WatchWarning = {
  code?: string;
  severity?: string;
  message?: string;
};

export type WatchResult = {
  watch_id: string;
  source_id: string;
  canonical_uri: string;
  adapter?: WatchAdapterRef;
  scope?: unknown;
  enabled: boolean;
  schedule: WatchSchedule;
  job?: unknown;
  latest_job?: unknown;
  warnings?: WatchWarning[];
};

export type WatchUpdateRequest = {
  enabled?: boolean;
  schedule?: WatchSchedule;
  options?: Record<string, unknown>;
  embed?: boolean;
  collection?: string;
};

// ---------------------------------------------------------------------------
// Memory (POST /v1/memories, /v1/memories/search, /v1/memories/context,
// GET/DELETE /v1/memories/{id})
// ---------------------------------------------------------------------------
//
// Mirrors crates/axon-services/src/memory/mapping.rs (MemoryItem) — the wire
// shape every /v1/memories* route returns/wraps. There is no GET list route;
// `search` (with an empty query the store returns the full recall-visible
// set) is the listing surface. Hand-typed rather than generated because the
// REST handlers return `serde_json::Value` (untyped in the OpenAPI schema);
// `RestMemoryRequest`/`RestMemoryNodeType` are the one part that is generated
// (see lib/axon-client.ts).
export type MemoryNodeType = 'decision' | 'fact' | 'preference' | 'task' | 'bug';

export const MEMORY_TYPE_OPTIONS: Array<{ value: MemoryNodeType; label: string }> = [
  { value: 'fact', label: 'Fact' },
  { value: 'decision', label: 'Decision' },
  { value: 'preference', label: 'Preference' },
  { value: 'task', label: 'Task' },
  { value: 'bug', label: 'Bug' }
];

export type MemoryItem = {
  id: string;
  memory_type: string;
  title: string;
  body?: string | null;
  project?: string | null;
  repo?: string | null;
  file?: string | null;
  workspace?: string | null;
  git_branch?: string | null;
  git_commit?: string | null;
  git_dirty?: boolean | null;
  cwd?: string | null;
  confidence: number;
  status: string;
  created_at: number;
  updated_at: number;
  last_seen_at: number;
  access_count: number;
  score?: number;
};

export type MemorySearchResponse = { memories: MemoryItem[] };
export type MemoryShowResponse = { memory: MemoryItem | null };
export type MemoryRememberResponse = { memory: MemoryItem };
