import type { components } from '../api/generated/axon-api';

export type PanelState = {
  setup_required: boolean;
  config_path: string;
};

export type ConfigResponse = {
  path: string;
  raw_toml: string;
  restart_required: boolean;
};

export type EnvConfigKeyState = {
  key: string;
  configured: boolean;
};

export type EnvConfigResponse = {
  keys: EnvConfigKeyState[];
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
    source_jobs?: ServiceJob[];
    extract_jobs?: ServiceJob[];
    watch_jobs?: ServiceJob[];
    prune_jobs?: ServiceJob[];
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
  kind?: 'source' | 'extract' | 'watch' | 'prune';
  error_text?: string | null;
  url?: string | null;
  source?: string | null;
  canonical_uri?: string | null;
  target?: string | null;
  source_type?: string | null;
  urls_json?: unknown;
};

export type ArtifactHandle = {
  artifact_id: string;
  bytes?: number;
  artifact_kind: string;
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

// UI projection of the generated SourcesResponse variants.
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

export type SourcesListResult = components['schemas']['SourcesResponse'];

// ---------------------------------------------------------------------------
// Watches — generated transport contracts.
// ---------------------------------------------------------------------------
export type WatchSchedule = components['schemas']['WatchSchedule'];
export type WatchSummary = components['schemas']['WatchSummary'];
export type WatchPage = components['schemas']['Page_WatchSummary'];
export type WatchUpdateRequest = components['schemas']['WatchUpdateRequest'];

// ---------------------------------------------------------------------------
// Memory — generated transport contracts.
// ---------------------------------------------------------------------------
export type MemoryNodeType = components['schemas']['RestMemoryNodeType'];

export const MEMORY_TYPE_OPTIONS: Array<{ value: MemoryNodeType; label: string }> = [
  { value: 'fact', label: 'Fact' },
  { value: 'decision', label: 'Decision' },
  { value: 'preference', label: 'Preference' },
  { value: 'task', label: 'Task' },
  { value: 'bug', label: 'Bug' }
];

export type MemoryItem = components['schemas']['MemoryItem'];
